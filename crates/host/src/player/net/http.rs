use reqwest::cookie::CookieStore;
use reqwest::header::{self, HeaderMap, HeaderName, HeaderValue};

use super::*;

const REDIRECT_LIMIT: usize = 10;

fn http_url(url: &Url) -> Result<(), NetError> {
    if matches!(url.scheme(), "http" | "https") {
        Ok(())
    } else {
        Err(NetError::Http(
            "only HTTP and HTTPS URLs are allowed".into(),
        ))
    }
}

fn headers(values: Vec<(String, String)>) -> Result<HeaderMap, NetError> {
    let mut headers = HeaderMap::new();
    for (name, value) in values {
        let name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|e| NetError::Http(e.to_string()))?;
        let value = HeaderValue::from_str(&value).map_err(|e| NetError::Http(e.to_string()))?;
        headers.append(name, value);
    }
    Ok(headers)
}

impl Net {
    pub(crate) fn fetch(
        &self,
        request: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, NetError>> + 'static {
        let http = self.http.clone();
        let cookies = self.cookies.clone();
        let deadline = tokio::time::Instant::now() + self.deadline;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.track(self.rt.spawn(async move {
            let mut response =
                match tokio::time::timeout_at(deadline, send(&http, &cookies, request)).await {
                    Ok(Ok(response)) => response,
                    Ok(Err(error)) => {
                        let _ = tx.send(Err(error));
                        return;
                    }
                    Err(_) => {
                        let _ = tx.send(Err(NetError::TimedOut));
                        return;
                    }
                };
            let (chunks_tx, chunks_rx) = async_channel::bounded(16);
            let failure = Arc::new(OnceLock::new());
            let head = HttpResponse {
                url: response.url().clone(),
                status: response.status().as_u16(),
                content_type: response
                    .headers()
                    .get(header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned),
                length: response.content_length(),
                body: HttpBody {
                    chunks: chunks_rx,
                    failure: failure.clone(),
                },
            };
            if tx.send(Ok(head)).is_err() {
                return;
            }
            loop {
                let chunk = tokio::time::timeout_at(deadline, response.chunk()).await;
                let chunk = match chunk {
                    Ok(Ok(Some(chunk))) => chunk.to_vec(),
                    Ok(Ok(None)) => break,
                    Ok(Err(error)) => {
                        let _ = failure.set(error.into());
                        break;
                    }
                    Err(_) => {
                        let _ = failure.set(NetError::TimedOut);
                        break;
                    }
                };
                match tokio::time::timeout_at(deadline, chunks_tx.send(chunk)).await {
                    Ok(Ok(())) => {}
                    Ok(_) => break,
                    Err(_) => {
                        let _ = failure.set(NetError::TimedOut);
                        break;
                    }
                }
            }
        }));
        async move { rx.await.unwrap_or(Err(NetError::Shutdown)) }
    }
}

async fn send(
    http: &reqwest::Client,
    cookies: &reqwest::cookie::Jar,
    request: HttpRequest,
) -> Result<reqwest::Response, NetError> {
    let HttpRequest {
        mut method,
        mut url,
        headers: values,
        mut body,
    } = request;
    let mut headers = headers(values)?;
    for hop in 0..=REDIRECT_LIMIT {
        http_url(&url)?;
        let mut hop_headers = headers.clone();
        let game = crate::net::identify(&url, &mut hop_headers);
        if !game {
            hop_headers.remove(header::COOKIE);
        } else if !hop_headers.contains_key(header::COOKIE)
            && let Some(value) = cookies.cookies(&url)
        {
            hop_headers.insert(header::COOKIE, value);
        }
        let mut builder = http
            .request(
                match method {
                    Method::Get => reqwest::Method::GET,
                    Method::Post => reqwest::Method::POST,
                },
                url.clone(),
            )
            .headers(hop_headers);
        if let Some((bytes, mime)) = &body {
            builder = builder
                .header(header::CONTENT_TYPE, mime)
                .body(bytes.clone());
        }
        let response = builder.send().await?;
        if game {
            cookies.set_cookies(
                &mut response.headers().get_all(header::SET_COOKIE).iter(),
                &url,
            );
        }
        let status = response.status().as_u16();
        if !matches!(status, 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        let Some(location) = response.headers().get(header::LOCATION) else {
            return Ok(response);
        };
        if hop == REDIRECT_LIMIT {
            return Err(NetError::Http("too many HTTP redirects".into()));
        }
        let location = location
            .to_str()
            .map_err(|e| NetError::Http(e.to_string()))?;
        let next = url
            .join(location)
            .map_err(|e| NetError::Http(e.to_string()))?;
        http_url(&next)?;
        if next.origin() != url.origin() {
            for name in [
                header::AUTHORIZATION,
                header::PROXY_AUTHORIZATION,
                header::COOKIE,
                header::HOST,
                header::ORIGIN,
                header::REFERER,
            ] {
                headers.remove(name);
            }
        }
        if status == 303 || (matches!(status, 301 | 302) && method == Method::Post) {
            method = Method::Get;
            body = None;
            for name in [
                header::CONTENT_TYPE,
                header::CONTENT_LENGTH,
                header::CONTENT_ENCODING,
                header::CONTENT_LANGUAGE,
                header::CONTENT_LOCATION,
                header::TRANSFER_ENCODING,
            ] {
                headers.remove(name);
            }
        }
        url = next;
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};

    #[tokio::test(start_paused = true)]
    async fn deadline_releases_the_request_when_the_body_queue_is_full() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = tokio::task::spawn_blocking(move || {
            let mut stream = accept(&listener);
            receive(&stream);
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 67108864\r\n\r\n")
                .unwrap();
            let chunk = [b'x'; 65536];
            for _ in 0..1024 {
                if let Err(error) = stream.write_all(&chunk) {
                    return error;
                }
            }
            panic!("the client consumed an unread 64 MiB response");
        });
        let mut net = Net::for_test(
            tokio::runtime::Handle::current(),
            vec![("game.aq.com", addr)],
        );
        net.deadline = Duration::from_millis(250);
        let response = net
            .fetch(HttpRequest {
                method: Method::Get,
                url: url("game.aq.com", addr, "/large"),
                headers: vec![],
                body: None,
            })
            .await
            .unwrap();
        let filling = std::time::Instant::now();
        while !response.body.chunks.is_full() {
            assert!(
                filling.elapsed() < Duration::from_secs(5),
                "the body queue never filled"
            );
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_millis(251)).await;
        let error = server.await.unwrap();
        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
            ),
            "expected the deadline to close the HTTP connection: {error:?}"
        );
        let result = async {
            while response.body.next_chunk().await?.is_some() {}
            Ok::<(), NetError>(())
        }
        .await;
        assert!(matches!(result, Err(NetError::TimedOut)));
    }

    const EXPECTED_USER_AGENT: &str = if cfg!(target_os = "macos") {
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_16_0) AppleWebKit/537.36 (KHTML, like Gecko) Safari/537.36"
    } else if cfg!(target_os = "windows") {
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Safari/537.36"
    } else {
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Safari/537.36"
    };

    #[derive(Debug)]
    struct Received {
        line: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    }

    impl Received {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.as_str())
        }
    }

    fn accept(listener: &TcpListener) -> TcpStream {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    return stream;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("expected an HTTP connection: {error}"),
            }
        }
    }

    fn receive(mut stream: &TcpStream) -> Received {
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        let head = String::from_utf8(head).unwrap();
        let mut lines = head.lines();
        let line = lines.next().unwrap().to_owned();
        let headers: Vec<_> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(n, v)| (n.to_ascii_lowercase(), v.trim().to_owned()))
            .collect();
        let len = headers
            .iter()
            .find(|(n, _)| n == "content-length")
            .map(|(_, v)| v.parse().unwrap())
            .unwrap_or(0);
        let mut body = vec![0; len];
        stream.read_exact(&mut body).unwrap();
        Received {
            line,
            headers,
            body,
        }
    }

    fn serve(
        responses: impl FnOnce(SocketAddr) -> Vec<String>,
    ) -> (SocketAddr, std::thread::JoinHandle<Vec<Received>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let responses = responses(addr);
        listener.set_nonblocking(true).unwrap();
        let thread = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let mut stream = accept(&listener);
                requests.push(receive(&stream));
                let _ = stream.write_all(response.as_bytes());
            }
            requests
        });
        (addr, thread)
    }

    fn answer(status: &str, headers: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",
            body.len()
        )
    }

    fn url(host: &str, addr: SocketAddr, path: &str) -> Url {
        Url::parse(&format!("http://{host}:{}{path}", addr.port())).unwrap()
    }

    fn load(
        runtime: &tokio::runtime::Runtime,
        net: &Net,
        method: Method,
        url: Url,
        headers: Vec<(String, String)>,
        body: Option<(Vec<u8>, String)>,
    ) -> Result<(Url, u16, Option<String>, Vec<u8>), NetError> {
        runtime.block_on(async {
            let response = net
                .fetch(HttpRequest {
                    method,
                    url,
                    headers,
                    body,
                })
                .await?;
            let mut body = Vec::new();
            while let Some(chunk) = response.body.next_chunk().await? {
                body.extend(chunk);
            }
            Ok((response.url, response.status, response.content_type, body))
        })
    }

    fn runtime_net(addr: SocketAddr) -> (tokio::runtime::Runtime, Net) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let net = Net::for_test(
            runtime.handle().clone(),
            vec![
                ("game.aq.com", addr),
                ("other.artix.com", addr),
                ("outside.example", addr),
            ],
        );
        (runtime, net)
    }

    fn overrides() -> Vec<(String, String)> {
        [
            ("UsEr-AgEnT", "spoofed"),
            ("ARTIXMODE", "spoofed"),
            ("Cookie", "caller=secret"),
            ("Authorization", "Bearer secret"),
            ("Proxy-Authorization", "secret"),
            ("Host", "spoof.example"),
            ("Origin", "http://game.aq.com"),
            ("Referer", "http://game.aq.com/private"),
        ]
        .into_iter()
        .map(|(n, v)| (n.into(), v.into()))
        .collect()
    }

    #[test]
    fn non_game_requests_do_not_carry_launcher_identity() {
        let (addr, server) = serve(|_| vec![answer("200 OK", "", "ok")]);
        let (runtime, net) = runtime_net(addr);
        assert_eq!(
            load(
                &runtime,
                &net,
                Method::Get,
                url("outside.example", addr, "/"),
                overrides(),
                None
            )
            .unwrap()
            .3,
            b"ok"
        );
        let requests = server.join().unwrap();
        assert_eq!(requests[0].header("user-agent"), None);
        assert_eq!(requests[0].header("artixmode"), None);
        assert_eq!(requests[0].header("cookie"), None);
    }

    #[test]
    fn redirects_recompute_identity_in_both_directions_and_scope_cookies() {
        let (addr, server) = serve(|addr| {
            vec![
                answer(
                    "302 Found",
                    &format!(
                        "Set-Cookie: game=private; Path=/\r\nLocation: {}\r\n",
                        url("outside.example", addr, "/away")
                    ),
                    "",
                ),
                answer(
                    "302 Found",
                    &format!(
                        "Set-Cookie: foreign=ignored; Domain=aq.com; Path=/\r\nLocation: {}\r\n",
                        url("game.aq.com", addr, "/back")
                    ),
                    "",
                ),
                answer(
                    "200 OK",
                    "Content-Type: text/plain; charset=utf-8\r\n",
                    "body",
                ),
            ]
        });
        let (runtime, net) = runtime_net(addr);
        let result = load(
            &runtime,
            &net,
            Method::Get,
            url("game.aq.com", addr, "/start"),
            overrides(),
            None,
        )
        .unwrap();
        assert_eq!(
            result,
            (
                url("game.aq.com", addr, "/back"),
                200,
                Some("text/plain; charset=utf-8".into()),
                b"body".to_vec()
            )
        );
        let requests = server.join().unwrap();
        assert_eq!(requests[0].header("user-agent"), Some(EXPECTED_USER_AGENT));
        assert_eq!(requests[0].header("artixmode"), Some("launcher"));
        assert_eq!(requests[0].header("cookie"), Some("caller=secret"));
        assert_eq!(requests[1].header("user-agent"), None);
        assert_eq!(requests[1].header("artixmode"), None);
        assert_eq!(requests[1].header("cookie"), None);
        for name in ["authorization", "proxy-authorization", "origin", "referer"] {
            assert_eq!(requests[1].header(name), None, "{name}");
        }
        assert_eq!(
            requests[1].header("host"),
            Some(format!("outside.example:{}", addr.port()).as_str())
        );
        assert_eq!(requests[2].header("user-agent"), Some(EXPECTED_USER_AGENT));
        assert_eq!(requests[2].header("artixmode"), Some("launcher"));
        assert_eq!(requests[2].header("cookie"), Some("game=private"));
    }

    #[test]
    fn cross_game_redirects_retain_launcher_identity_without_origin_credentials() {
        let (addr, server) = serve(|addr| {
            vec![
                answer(
                    "307 Temporary Redirect",
                    &format!("Location: {}\r\n", url("other.artix.com", addr, "/final")),
                    "",
                ),
                answer("200 OK", "", ""),
            ]
        });
        let (runtime, net) = runtime_net(addr);
        load(
            &runtime,
            &net,
            Method::Get,
            url("game.aq.com", addr, "/start"),
            overrides(),
            None,
        )
        .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests[1].header("user-agent"), Some(EXPECTED_USER_AGENT));
        assert_eq!(requests[1].header("artixmode"), Some("launcher"));
        assert_eq!(requests[1].header("authorization"), None);
        assert_eq!(requests[1].header("cookie"), None);
    }

    #[test]
    fn post_redirects_change_or_preserve_the_method_body_and_headers() {
        for status in [301, 302, 303, 307, 308] {
            let (addr, server) = serve(|_| {
                vec![
                    answer(&format!("{status} Redirect"), "Location: /final\r\n", ""),
                    answer("200 OK", "", ""),
                ]
            });
            let (runtime, net) = runtime_net(addr);
            load(
                &runtime,
                &net,
                Method::Post,
                url("game.aq.com", addr, "/start"),
                vec![
                    ("Content-Encoding".into(), "identity".into()),
                    ("Content-Language".into(), "en".into()),
                    ("Content-Location".into(), "/body".into()),
                ],
                Some((b"message".to_vec(), "application/test".into())),
            )
            .unwrap();
            let requests = server.join().unwrap();
            assert_eq!(requests[0].line, "POST /start HTTP/1.1");
            assert_eq!(requests[0].body, b"message");
            if status == 307 || status == 308 {
                assert_eq!(requests[1].line, "POST /final HTTP/1.1");
                assert_eq!(requests[1].body, b"message");
                assert_eq!(requests[1].header("content-type"), Some("application/test"));
                assert_eq!(requests[1].header("content-encoding"), Some("identity"));
                assert_eq!(requests[1].header("content-language"), Some("en"));
                assert_eq!(requests[1].header("content-location"), Some("/body"));
            } else {
                assert_eq!(requests[1].line, "GET /final HTTP/1.1");
                assert_eq!(requests[1].body, b"");
                for name in [
                    "content-type",
                    "content-length",
                    "content-encoding",
                    "content-language",
                    "content-location",
                    "transfer-encoding",
                ] {
                    assert_eq!(requests[1].header(name), None);
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn redirect_and_body_share_one_deadline() {
        for send_headers in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let (first_tx, first_rx) = tokio::sync::oneshot::channel();
            let (second_tx, second_rx) = tokio::sync::oneshot::channel();
            let (head_tx, head_rx) = tokio::sync::oneshot::channel();
            let (redirect_tx, redirect_rx) = std::sync::mpsc::channel();
            // Blocking socket I/O keeps Tokio's paused clock from auto-advancing.
            let server = tokio::task::spawn_blocking(move || {
                let mut first = accept(&listener);
                assert_eq!(receive(&first).line, "GET /start HTTP/1.1");
                first_tx.send(()).unwrap();
                redirect_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                first
                    .write_all(answer("302 Found", "Location: /final\r\n", "").as_bytes())
                    .unwrap();
                drop(first);
                let mut second = accept(&listener);
                assert_eq!(receive(&second).line, "GET /final HTTP/1.1");
                if send_headers {
                    second
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 4\r\n\r\n",
                        )
                        .unwrap();
                }
                second_tx.send(()).unwrap();
                second
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                assert_eq!(second.read(&mut [0]).unwrap(), 0);
            });
            let mut net = Net::for_test(
                tokio::runtime::Handle::current(),
                vec![("game.aq.com", addr)],
            );
            net.deadline = Duration::from_millis(260);
            let start = tokio::time::Instant::now();
            let request = tokio::spawn(async move {
                let response = net
                    .fetch(HttpRequest {
                        method: Method::Get,
                        url: url("game.aq.com", addr, "/start"),
                        headers: vec![],
                        body: None,
                    })
                    .await?;
                head_tx.send(()).unwrap();
                let mut body = Vec::new();
                while let Some(chunk) = response.body.next_chunk().await? {
                    body.extend(chunk);
                }
                Ok::<_, NetError>(body)
            });
            first_rx.await.unwrap();
            tokio::time::advance(Duration::from_millis(160)).await;
            redirect_tx.send(()).unwrap();
            second_rx.await.unwrap();
            if send_headers {
                head_rx.await.unwrap();
            }
            tokio::time::advance(Duration::from_millis(99)).await;
            assert!(
                !request.is_finished(),
                "the request expired before its deadline"
            );
            tokio::time::advance(Duration::from_millis(2)).await;
            assert!(matches!(request.await.unwrap(), Err(NetError::TimedOut)));
            assert_eq!(start.elapsed(), Duration::from_millis(261));
            server.await.unwrap();
        }
    }

    #[test]
    fn redirects_are_bounded_and_reject_non_http_targets() {
        let (addr, server) = serve(|_| {
            vec![answer(
                "302 Found",
                "Location: file:///private/test\r\n",
                "",
            )]
        });
        let (runtime, net) = runtime_net(addr);
        assert!(
            matches!(load(&runtime, &net, Method::Get, url("game.aq.com", addr, "/"), vec![], None), Err(NetError::Http(message)) if message.contains("only HTTP"))
        );
        server.join().unwrap();
        assert!(
            matches!(load(&runtime, &net, Method::Get, Url::parse("file:///private/test").unwrap(), vec![], None), Err(NetError::Http(message)) if message.contains("only HTTP"))
        );
        let (addr, server) = serve(|_| {
            (0..=10)
                .map(|_| answer("302 Found", "Location: /loop\r\n", ""))
                .collect()
        });
        let (runtime, net) = runtime_net(addr);
        assert!(
            matches!(load(&runtime, &net, Method::Get, url("game.aq.com", addr, "/loop"), vec![], None), Err(NetError::Http(message)) if message.contains("too many"))
        );
        assert_eq!(server.join().unwrap().len(), 11);
    }

    #[test]
    fn non_game_cookies_are_neither_stored_nor_replayed() {
        let (addr, server) = serve(|_| {
            vec![
                answer("200 OK", "Set-Cookie: external=private; Path=/\r\n", ""),
                answer("200 OK", "", ""),
            ]
        });
        let (runtime, net) = runtime_net(addr);
        for _ in 0..2 {
            load(
                &runtime,
                &net,
                Method::Get,
                url("outside.example", addr, "/"),
                vec![],
                None,
            )
            .unwrap();
        }
        let requests = server.join().unwrap();
        assert_eq!(requests[1].header("cookie"), None);
    }

    #[test]
    fn same_origin_redirect_retains_game_cookies_and_authorization() {
        let (addr, server) = serve(|_| {
            vec![
                answer(
                    "302 Found",
                    "Set-Cookie: game=private; Path=/\r\nLocation: /final\r\n",
                    "",
                ),
                answer("200 OK", "", ""),
            ]
        });
        let (runtime, net) = runtime_net(addr);
        load(
            &runtime,
            &net,
            Method::Get,
            url("game.aq.com", addr, "/"),
            vec![("Authorization".into(), "Bearer retained".into())],
            None,
        )
        .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests[1].header("cookie"), Some("game=private"));
        assert_eq!(requests[1].header("authorization"), Some("Bearer retained"));
    }
}
