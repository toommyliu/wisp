mod common;

use common::*;
use lucent_host::{
    Host, Session,
    scripting::{Kill, RuntimeError, ScriptRun},
    testing,
};
use lucent_protocol::script::{ScriptDialogResponse, ScriptOutcome};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Request> {
    stream.set_read_timeout(Some(TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(TIMEOUT)).unwrap();
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        bytes.push(byte[0]);
    }
    let text: String = bytes.into_iter().map(char::from).collect();
    let mut lines = text.split("\r\n");
    let mut first = lines.next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_owned();
    let path = first.next().unwrap().to_owned();
    let headers: BTreeMap<_, _> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let size = headers
        .get("content-length")
        .map(|value| value.parse().unwrap())
        .unwrap_or(0);
    let mut body = vec![0; size];
    stream.read_exact(&mut body)?;
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

fn respond(stream: &mut TcpStream, status: &str, headers: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

struct Server {
    url: String,
    requests: mpsc::Receiver<Request>,
    stopping: Arc<AtomicBool>,
    streams: Arc<Mutex<Vec<TcpStream>>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn start(handler: impl Fn(&mut TcpStream, &Request) + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let streams = Arc::new(Mutex::new(Vec::new()));
        let sockets = streams.clone();
        let (sent, requests) = mpsc::channel();
        let handler = Arc::new(handler);
        let thread = thread::spawn(move || {
            let mut workers = Vec::new();
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                if stop.load(Ordering::Acquire) {
                    break;
                }
                sockets.lock().unwrap().push(stream.try_clone().unwrap());
                let handler = handler.clone();
                let sent = sent.clone();
                workers.push(thread::spawn(move || {
                    let Ok(request) = read_request(&mut stream) else {
                        return;
                    };
                    let _ = sent.send(request.clone());
                    handler(&mut stream, &request);
                }));
            }
            for stream in sockets.lock().unwrap().iter() {
                let _ = stream.shutdown(Shutdown::Both);
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            url: format!("http://{address}"),
            requests,
            stopping,
            streams,
            thread: Some(thread),
        }
    }

    fn request(&self) -> Request {
        self.requests.recv_timeout(TIMEOUT).unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        for stream in self.streams.lock().unwrap().iter() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        let _ = TcpStream::connect(self.url.trim_start_matches("http://"));
        self.thread.take().unwrap().join().unwrap();
    }
}

struct HttpRun {
    session: Session,
    run: ScriptRun,
}
impl HttpRun {
    fn start(host: &Host, url: &str, body: &str) -> Self {
        let (session, link) = testing::start_probe(host, ready_movie());
        log_in(&link);
        let source = format!(
            r#"
            import * as http from 'lucent/http';
            import * as fs from 'lucent/filesystem';
            import * as script from 'lucent/script';
            import {{ Effect }} from 'effect';
            const url = {};
            const result = effect => effect.pipe(Effect.map(value => ({{ok:true, value}})), Effect.catch(error => Effect.succeed({{ok:false, reason:error.reason, detail:error.detail, url:error.url}})));
            export default function* () {{
                const value = yield* Effect.gen(function* () {{ {body} }});
                yield* script.stop(JSON.stringify(value));
            }}
        "#,
            json!(url)
        );
        let run =
            ScriptRun::start(link, loose(&[("http.js", &source)]), Default::default()).unwrap();
        Self { session, run }
    }

    fn result(&self) -> Value {
        match self.run.wait(TIMEOUT).unwrap() {
            ScriptOutcome::StoppedByScript {
                reason: Some(value),
            } => serde_json::from_str(&value).unwrap(),
            outcome => panic!("{outcome:?}"),
        }
    }
}

fn run(url: &str, body: &str) -> Value {
    let host = testing::headless_host();
    HttpRun::start(&host, url, body).result()
}

#[test]
fn get_and_post_preserve_body_bytes_and_default_content_types() {
    let server = Server::start(|stream, _| respond(stream, "200 OK", "X-Reply: yes\r\n", b"hello"));
    assert_eq!(
        run(
            &server.url,
            r#"
        const bodies = [undefined, 'héllo', new URLSearchParams([['q','a b']]), new Uint8Array([9,0,255]).subarray(1), new Uint8Array([0,255]).buffer];
        const values = [];
        for (const body of bodies) {
            const response = yield* http.request(url, body === undefined ? {} : {method:'POST',body});
            values.push([response.status, response.statusText, response.ok, response.headers.get('x-reply'), yield* response.text()]);
        }
        return values;
    "#
        ),
        json!(vec![json!([200, "OK", true, "yes", "hello"]); 5])
    );
    let requests: Vec<_> = (0..5).map(|_| server.request()).collect();
    assert_eq!(requests[0].method, "GET");
    for request in &requests[1..] {
        assert_eq!(request.method, "POST");
    }
    assert_eq!(
        requests.iter().map(|r| r.body.clone()).collect::<Vec<_>>(),
        [
            vec![],
            "héllo".as_bytes().to_vec(),
            b"q=a+b".to_vec(),
            vec![0, 255],
            vec![0, 255]
        ]
    );
    assert_eq!(
        requests
            .iter()
            .map(|r| r.headers.get("content-type").map(String::as_str))
            .collect::<Vec<_>>(),
        [
            None,
            Some("text/plain;charset=UTF-8"),
            Some("application/x-www-form-urlencoded;charset=UTF-8"),
            None,
            None
        ]
    );
}

#[test]
fn redirects_follow_manual_error_and_hop_limits() {
    let server = Server::start(|stream, request| {
        if request.path == "/done" {
            respond(stream, "200 OK", "", b"done");
        } else {
            respond(stream, "302 Found", "Location: /done\r\n", b"redirect");
        }
    });
    assert_eq!(
        run(
            &server.url,
            r#"
        const followed = yield* http.request(url);
        const manual = yield* http.request(url, {redirect:'manual', maxRedirects:0});
        return [followed.url.endsWith('/done'), yield* followed.text(), manual.status, yield* manual.text(),
            (yield* result(http.request(url, {redirect:'error'}))).reason,
            (yield* result(http.request(url, {maxRedirects:0}))).reason];
    "#
        ),
        json!([true, "done", 302, "redirect", "redirect", "redirect"])
    );
    let loop_server = Server::start(|stream, _| {
        respond(
            stream,
            "307 Temporary Redirect",
            "Location: /again\r\n",
            b"",
        )
    });
    assert_eq!(
        run(
            &loop_server.url,
            "return (yield* result(http.request(url, {maxRedirects:2}))).reason;"
        ),
        json!("redirect")
    );
    for _ in 0..3 {
        loop_server.request();
    }
    assert!(loop_server.requests.try_recv().is_err());
}

#[test]
fn redirects_rewrite_post_and_303_methods_and_preserve_307_and_308_bodies() {
    let server = Server::start(|stream, request| {
        if request.path == "/done" {
            respond(stream, "200 OK", "", b"done");
        } else {
            respond(
                stream,
                &format!("{} Redirect", &request.path[1..]),
                "Location: /done\r\n",
                b"",
            );
        }
    });
    assert_eq!(
        run(
            &server.url,
            r#"
        for (const [status, method] of [[301,'POST'],[302,'POST'],[303,'PATCH'],[307,'POST'],[308,'PATCH']]) {
            yield* http.request(`${url}/${status}`, {method,body:'payload',headers:{'content-language':'en','content-encoding':'identity','content-location':'/body'}});
        }
        return 'done';
    "#
        ),
        json!("done")
    );
    for (method, body) in [
        ("GET", b"".as_slice()),
        ("GET", b""),
        ("GET", b""),
        ("POST", b"payload"),
        ("PATCH", b"payload"),
    ] {
        server.request();
        let request = server.request();
        assert_eq!(request.method, method);
        assert_eq!(request.body, body);
        if method == "GET" {
            for name in [
                "content-type",
                "content-length",
                "content-encoding",
                "content-language",
                "content-location",
                "transfer-encoding",
            ] {
                assert!(!request.headers.contains_key(name), "{name}");
            }
        }
    }
}

#[test]
fn cross_origin_redirects_strip_credentials_but_same_origin_keeps_them() {
    let destination = Server::start(|stream, _| respond(stream, "200 OK", "", b"done"));
    let next = destination.url.clone();
    let server = Server::start(move |stream, request| {
        if request.path == "/same" {
            respond(stream, "200 OK", "", b"done");
        } else {
            respond(
                stream,
                "302 Found",
                &format!(
                    "Location: {}\r\n",
                    if request.path == "/cross" {
                        next.as_str()
                    } else {
                        "/same"
                    }
                ),
                b"",
            );
        }
    });
    assert_eq!(
        run(
            &server.url,
            r#"
        const headers = {Authorization:'Bearer secret',Cookie:'session=secret',Host:'custom.invalid','Proxy-Authorization':'Basic secret','X-Keep':'yes'};
        yield* http.request(url, {headers});
        yield* http.request(`${url}/cross`, {headers});
        return 'done';
    "#
        ),
        json!("done")
    );
    server.request();
    let same = server.request();
    assert_eq!(same.headers.get("authorization").unwrap(), "Bearer secret");
    assert_eq!(same.headers.get("cookie").unwrap(), "session=secret");
    assert_eq!(same.headers.get("host").unwrap(), "custom.invalid");
    assert_eq!(
        same.headers.get("proxy-authorization").unwrap(),
        "Basic secret"
    );
    let cross = destination.request();
    for name in ["authorization", "cookie", "proxy-authorization"] {
        assert!(!cross.headers.contains_key(name), "{name}");
    }
    assert_eq!(
        cross.headers.get("host").unwrap(),
        destination.url.trim_start_matches("http://")
    );
    assert_eq!(cross.headers.get("x-keep").unwrap(), "yes");
}

#[test]
fn timeout_covers_response_body_and_closes_the_connection() {
    let (closed, closing) = mpsc::channel();
    let server = Server::start(move |stream, request| {
        if request.path == "/" {
            respond(stream, "302 Found", "Location: /slow\r\n", b"");
            return;
        }
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n").unwrap();
        let mut byte = [0];
        let _ = closed.send(stream.read(&mut byte).map(|n| n == 0).unwrap_or(false));
    });
    assert_eq!(
        run(
            &server.url,
            "const error = yield* result(http.request(url, {timeout:1000})); return [error.reason, new URL(error.url).pathname];"
        ),
        json!(["timeout", "/slow"])
    );
    assert!(closing.recv_timeout(TIMEOUT).unwrap());
}

#[test]
fn oversized_content_length_is_rejected_before_reading_the_body() {
    let server = Server::start(|stream, _| {
        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 8388609\r\n\r\n");
        let mut byte = [0];
        let _ = stream.read(&mut byte);
    });
    assert_eq!(
        run(
            &server.url,
            "return (yield* result(http.request(url))).reason;"
        ),
        json!("too-large")
    );
}

#[test]
fn streamed_bodies_stop_at_the_byte_limit() {
    let server = Server::start(|stream, _| {
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n"
        );
        let chunk = vec![b'x'; 65536];
        for _ in 0..129 {
            if write!(stream, "10000\r\n")
                .and_then(|()| stream.write_all(&chunk))
                .and_then(|()| stream.write_all(b"\r\n"))
                .is_err()
            {
                return;
            }
        }
        let _ = stream.write_all(b"0\r\n\r\n");
    });
    assert_eq!(
        run(
            &server.url,
            "return (yield* result(http.request(url))).reason;"
        ),
        json!("too-large")
    );
}

#[test]
fn signal_abort_returns_the_aborted_error_and_closes_the_connection() {
    let (closed, closing) = mpsc::channel();
    let server = Server::start(move |stream, _| {
        let mut byte = [0];
        let _ = closed.send(stream.read(&mut byte).map(|n| n == 0).unwrap_or(false));
    });
    let host = testing::headless_host();
    let task = HttpRun::start(
        &host,
        &server.url,
        r#"
        const controller = new AbortController();
        return yield* Effect.raceFirst(result(http.request(url, {signal:controller.signal})), Effect.gen(function* () {
            yield* script.alert('abort'); controller.abort(); yield* Effect.tryPromise(() => new Promise(() => {}));
        }));
    "#,
    );
    server.request();
    wait_dialog(&task);
    let dialog = task.session.script_dialog().unwrap();
    task.session
        .respond_dialog(dialog.id, ScriptDialogResponse::Dismissed);
    assert_eq!(
        task.result(),
        json!({"ok":false,"reason":"aborted","detail":"HTTP request was aborted.","url":format!("{}/", server.url)})
    );
    assert!(closing.recv_timeout(TIMEOUT).unwrap());
}

fn wait_dialog(task: &HttpRun) {
    struct Wake(mpsc::Sender<()>);
    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }
    let (sent, changed) = mpsc::channel();
    task.session
        .watch(std::task::Waker::from(Arc::new(Wake(sent))));
    let deadline = Instant::now() + TIMEOUT;
    while task.session.script_dialog().is_none() {
        changed
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}

#[test]
fn stopping_killing_and_ending_runs_close_inflight_connections() {
    for mode in ["stop", "kill", "end"] {
        let (closed, closing) = mpsc::channel();
        let server = Server::start(move |stream, _| {
            let mut byte = [0];
            let _ = closed.send(stream.read(&mut byte).map(|n| n == 0).unwrap_or(false));
        });
        let host = testing::headless_host();
        let task = HttpRun::start(
            &host,
            &server.url,
            r#"
            return yield* Effect.raceFirst(http.request(url), Effect.gen(function* () { yield* script.alert('end'); return 'ended'; }));
        "#,
        );
        server.request();
        let began = Instant::now();
        match mode {
            "stop" => {
                task.run.stop_script();
                assert_eq!(
                    task.run.wait(Duration::from_secs(2)).unwrap(),
                    ScriptOutcome::Stopped { reason: None }
                );
            }
            "kill" => {
                task.run.kill();
                assert!(matches!(
                    task.run.wait(Duration::from_secs(2)),
                    Err(RuntimeError::Killed(Kill::Requested))
                ));
            }
            _ => {
                wait_dialog(&task);
                let dialog = task.session.script_dialog().unwrap();
                task.session
                    .respond_dialog(dialog.id, ScriptDialogResponse::Dismissed);
                assert_eq!(task.result(), json!("ended"));
            }
        }
        assert!(closing.recv_timeout(Duration::from_secs(2)).unwrap());
        assert!(began.elapsed() < TIMEOUT);
    }
}

#[test]
fn requests_do_not_inherit_user_agent_or_store_response_cookies() {
    let server = Server::start(|stream, _| {
        respond(
            stream,
            "200 OK",
            "Set-Cookie: session=private; Path=/\r\n",
            b"ok",
        )
    });
    let mut host = testing::headless_host();
    host.resolve_script_http(
        "game.aq.com",
        server.url.trim_start_matches("http://").parse().unwrap(),
    );
    let url = server.url.replace("127.0.0.1", "game.aq.com");
    assert_eq!(
        HttpRun::start(
            &host,
            &url,
            "yield* http.request(url); yield* http.request(url); return 'done';"
        )
        .result(),
        json!("done")
    );
    for _ in 0..2 {
        let request = server.request();
        assert!(!request.headers.contains_key("user-agent"));
        assert!(!request.headers.contains_key("cookie"));
    }
}

#[test]
fn response_bytes_are_exact_and_bodies_are_consumed_once() {
    let server = Server::start(|stream, _| respond(stream, "200 OK", "", &[0, 255, 128, 1]));
    assert_eq!(
        run(
            &server.url,
            r#"
        const response = yield* http.request(url);
        const before = response.bodyUsed;
        const bytes = yield* response.arrayBuffer();
        return [bytes instanceof ArrayBuffer, [...new Uint8Array(bytes)], before, response.bodyUsed, (yield* result(response.text())).reason];
    "#
        ),
        json!([true, [0, 255, 128, 1], false, true, "body"])
    );
}

#[test]
fn many_response_headers_iterate_in_a_single_turn() {
    let headers = (0..98)
        .map(|i| format!("X-Test-{i:04}: {i}\r\n"))
        .collect::<String>();
    let server = Server::start(move |stream, _| respond(stream, "200 OK", &headers, b"ok"));
    assert_eq!(
        run(
            &server.url,
            r#"
        const response = yield* http.request(url);
        let count = 0; let sum = 0;
        for (const [name,value] of response.headers) if (name.startsWith('x-test-')) { count++; sum += Number(value); }
        return [count,sum];
    "#
        ),
        json!([98, 4753])
    );
}

#[test]
fn extension_methods_head_and_protocol_upgrades_follow_the_contract() {
    let server = Server::start(|stream, request| {
        if request.path == "/upgrade" {
            let _ = stream.write_all(
                b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: test\r\n\r\n",
            );
        } else if request.method == "HEAD" {
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 8388609\r\n\r\n",
            );
        } else {
            respond(stream, "200 OK", "", b"ok");
        }
    });
    assert_eq!(
        run(
            &server.url,
            r#"
        const extension = yield* http.request(url, {method:'CUSTOM'});
        const head = yield* http.request(url, {method:'HEAD'});
        return [extension.status, yield* head.text(), (yield* result(http.request(url, {method:'CONNECT'}))).reason, (yield* result(http.request(`${url}/upgrade`))).reason];
    "#
        ),
        json!([200, "", "request", "request"])
    );
    assert_eq!(server.request().method, "CUSTOM");
}

#[test]
fn the_exact_body_limit_succeeds_without_json_byte_arrays() {
    let server =
        Server::start(|stream, _| respond(stream, "200 OK", "", &vec![255; 8 * 1024 * 1024]));
    assert_eq!(
        run(
            &server.url,
            r#"
        const response = yield* http.request(url);
        const bytes = new Uint8Array(yield* response.arrayBuffer());
        return [bytes.length, bytes[0], bytes[bytes.length-1]];
    "#
        ),
        json!([8388608, 255, 255])
    );
}

#[test]
fn http_files_and_dialogs_share_the_pending_request_limit() {
    let (closed, closing) = mpsc::channel();
    let server = Server::start(move |stream, _| {
        let mut byte = [0];
        let _ = closed.send(stream.read(&mut byte).map(|n| n == 0).unwrap_or(false));
    });
    let host = testing::headless_host();
    let task = HttpRun::start(
        &host,
        &server.url,
        r#"
        const waiting = [...Array.from({length:15}, () => http.request(url)), script.alert('held')]
            .reduce((left,right) => Effect.raceFirst(left,right));
        const full = Effect.tryPromise(() => Promise.resolve()).pipe(Effect.flatMap(() =>
            fs.exists('missing').pipe(Effect.map(() => 'accepted'), Effect.catch(error => Effect.succeed(error.reason)))));
        return yield* Effect.raceFirst(waiting,full);
    "#,
    );
    assert_eq!(task.result(), json!("busy"));
    drop(closing);
}

#[test]
fn redirect_urls_and_request_options_reject_invalid_values() {
    let server = Server::start(|stream, request| {
        let location = if request.path == "/credentials" {
            "http://user:password@127.0.0.1/"
        } else {
            "file:///private"
        };
        respond(
            stream,
            "302 Found",
            &format!("Location: {location}\r\n"),
            b"",
        );
    });
    assert_eq!(
        run(
            &server.url,
            r#"
        const reasons = [];
        for (const [input, options] of [
            [url,{method:'GET',body:'no'}], [url,{method:'HEAD',body:'no'}], [url,{method:'bad method'}],
            [url,{maxRedirects:-1}], [url,{timeout:0}], [url,{redirect:'invalid'}],
            ['file:///private',{}], ['http://user:password@127.0.0.1/',{}],
            [`${url}/credentials`,{}], [url,{}]
        ]) reasons.push((yield* result(http.request(input,options))).reason);
        return reasons;
    "#
        ),
        json!([
            "request", "request", "request", "request", "timeout", "request", "request", "request",
            "redirect", "redirect"
        ])
    );
}

#[test]
fn response_metadata_keeps_custom_status_text_and_duplicate_headers() {
    let server = Server::start(|stream, _| {
        respond(
            stream,
            "299 Custom result",
            "X-Duplicate: first\r\nX-Duplicate: second\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n",
            br#"{"value":2}"#,
        )
    });
    assert_eq!(
        run(
            &server.url,
            r#"
        const response = yield* http.request(url);
        return [response.status, response.statusText, response.headers.get('x-duplicate'), response.headers.getSetCookie(), yield* response.json()];
    "#
        ),
        json!([299,"Custom result","first, second",["a=1","b=2"],{"value":2}])
    );
}

#[test]
fn byte_string_headers_and_redirect_locations_preserve_latin1() {
    let server = Server::start(|stream, request| {
        let response = if request.path == "/" {
            b"HTTP/1.1 302 Found\r\nConnection: close\r\nContent-Length: 0\r\nLocation: /caf\xe9\r\n\r\n".as_slice()
        } else {
            b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 0\r\nX-Latin: caf\xe9\r\n\r\n"
                .as_slice()
        };
        let _ = stream.write_all(response);
    });
    assert_eq!(
        run(
            &server.url,
            r#"
        const response = yield* http.request(url, {headers:{'X-Latin':'café'}});
        return [response.headers.get('x-latin'), response.url.toLowerCase().endsWith('/caf%c3%a9')];
    "#
        ),
        json!(["café", true])
    );
    assert_eq!(server.request().headers.get("x-latin").unwrap(), "café");
    assert_eq!(server.request().path.to_ascii_lowercase(), "/caf%c3%a9");
}
