use super::*;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;
use tokio::io::DuplexStream;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::time::advance;

const VER_CHK: &[u8] = b"<msg t='sys'><body action='verChk' r='0'><ver v='157' /></body></msg>\0";
const API_OK: &[u8] = b"<msg t='sys'><body action='apiOK' r='0'></body></msg>\0";

type Script = Box<dyn Fn(usize) -> Vec<Step> + Send + Sync>;

struct Fake {
    respond: Box<dyn Fn(usize) -> Result<Arc<Listing>, LoadError> + Send + Sync>,
    script: Script,
    requests: AtomicUsize,
    sockets: Mutex<Vec<Socket>>,
}

#[derive(Clone, Copy)]
enum Step {
    Reply(Duration),
    Close,
    Flood,
}

fn replies(delays_ms: [u64; 4]) -> Vec<Step> {
    delays_ms
        .map(|delay| Step::Reply(Duration::from_millis(delay)))
        .to_vec()
}

struct Socket {
    connect: Option<oneshot::Sender<()>>,
    closed: Arc<AtomicBool>,
}

struct Closes(Arc<AtomicBool>);

impl Drop for Closes {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl Fake {
    fn new(
        respond: impl Fn(usize) -> Result<Arc<Listing>, LoadError> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Self::scripted(respond, |_| replies([0; 4]))
    }

    fn scripted(
        respond: impl Fn(usize) -> Result<Arc<Listing>, LoadError> + Send + Sync + 'static,
        script: impl Fn(usize) -> Vec<Step> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            respond: Box::new(respond),
            script: Box::new(script),
            requests: AtomicUsize::new(0),
            sockets: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    fn dialed(&self) -> usize {
        self.sockets.lock().unwrap().len()
    }

    fn open(&self) -> Vec<usize> {
        let sockets = self.sockets.lock().unwrap();
        (0..sockets.len())
            .filter(|&index| !sockets[index].closed.load(Ordering::SeqCst))
            .collect()
    }

    fn connect(&self, index: usize) {
        if let Some(connect) = self.sockets.lock().unwrap()[index].connect.take() {
            let _ = connect.send(());
        }
    }

    fn refuse(&self, index: usize) {
        self.sockets.lock().unwrap()[index].connect.take();
    }
}

async fn answer(mut server: DuplexStream, script: Vec<Step>, _closes: Closes) {
    let mut steps = script.into_iter();
    let mut request = Vec::new();
    let mut buffer = [0; 256];
    loop {
        let read = match server.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        for &byte in &buffer[..read] {
            request.push(byte);
            if byte != 0 {
                continue;
            }
            assert_eq!(std::mem::take(&mut request), VER_CHK);
            let sent = match steps.next() {
                Some(Step::Reply(delay)) => {
                    tokio::time::sleep(delay).await;
                    server.write_all(API_OK).await
                }
                Some(Step::Close) => server.shutdown().await,
                Some(Step::Flood) => server.write_all(&[b'x'; 4097]).await,
                None => Ok(()),
            };
            if sent.is_err() {
                return;
            }
        }
    }
}

impl Network for Fake {
    fn fetch(&self) -> BoxFuture<Result<Arc<Listing>, LoadError>> {
        let request = self.requests.fetch_add(1, Ordering::SeqCst) + 1;
        let outcome = (self.respond)(request);
        Box::pin(async move { outcome })
    }

    fn dial(&self, _host: &GameHost, _port: u16) -> BoxFuture<io::Result<Box<dyn Connection>>> {
        let (connect, connected) = oneshot::channel();
        let closed = Arc::new(AtomicBool::new(false));
        let mut sockets = self.sockets.lock().unwrap();
        let script = (self.script)(sockets.len());
        sockets.push(Socket {
            connect: Some(connect),
            closed: closed.clone(),
        });
        let (client, server) = tokio::io::duplex(8192);
        tokio::spawn(answer(server, script, Closes(closed)));
        Box::pin(async move {
            connected
                .await
                .map_err(|_| io::Error::from(io::ErrorKind::ConnectionRefused))?;
            Ok(Box::new(client) as Box<dyn Connection>)
        })
    }
}

struct Wall {
    started: tokio::time::Instant,
    slept: Mutex<Duration>,
}

impl Wall {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + self.started.elapsed() + *self.slept.lock().unwrap()
    }

    fn sleep(&self, duration: Duration) {
        *self.slept.lock().unwrap() += duration;
    }
}

fn serving(fake: &Arc<Fake>) -> (LoginServers, Arc<Wall>) {
    let wall = Arc::new(Wall {
        started: tokio::time::Instant::now(),
        slept: Mutex::new(Duration::ZERO),
    });
    let clock = wall.clone();
    let servers = LoginServers::serving(&Handle::current(), fake.clone(), move || clock.now());
    (servers, wall)
}

async fn until(condition: impl Fn() -> bool) {
    for _ in 0..10_000 {
        if condition() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("the condition never held");
}

fn row(name: &str, online: bool) -> Value {
    json!({
        "sName": name, "sIP": "sock7.aq.com", "iPort": 5588, "bOnline": u8::from(online),
        "bUpg": 0, "iCount": 10, "iMax": 1000, "sLang": "en",
    })
}

fn listing(rows: Value) -> Arc<Listing> {
    Arc::new(Listing::parse(&serde_json::to_vec(&rows).unwrap()).unwrap())
}

fn ping(name: &str, status: PingStatus) -> Ping {
    Ping {
        server_name: name.into(),
        status,
    }
}

fn serve_once(status: &str, body: Vec<u8>) -> (Url, std::thread::JoinHandle<Vec<String>>) {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend(body);
    serve_raw(response)
}

fn serve_raw(response: Vec<u8>) -> (Url, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut lines = Vec::new();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            lines.push(line.trim_end().to_ascii_lowercase());
        }
        let _ = stream.write_all(&response);
        lines
    });
    let url = Url::parse(&format!(
        "http://game.aq.com:{}/game/api/data/servers",
        addr.port()
    ))
    .unwrap();
    (url, thread)
}

fn test_client(url: &Url) -> reqwest::Client {
    let port = url.port().unwrap();
    client(
        reqwest::Client::builder()
            .no_proxy()
            .resolve("game.aq.com", SocketAddr::from(([127, 0, 0, 1], port))),
    )
    .unwrap()
}

#[tokio::test]
async fn decodes_server_data_and_retains_typed_http_failures() {
    let value = row("Artix", true);
    let (url, server) = serve_once("200 OK", serde_json::to_vec(&json!([value])).unwrap());
    let decoded = request(&test_client(&url), &url).await.unwrap();
    assert_eq!(
        decoded.servers,
        [Server {
            name: "Artix".into(),
            language: "en".into(),
            online: true,
            upgrade: false,
            player_count: 10.0,
            max_players: 1000.0,
        }]
    );
    let head = server.join().unwrap();
    assert!(head.contains(&"accept: application/json".to_owned()));
    assert!(head.contains(&"artixmode: launcher".to_owned()));
    assert!(head.contains(&format!(
        "user-agent: {}",
        crate::net::USER_AGENT.to_ascii_lowercase()
    )));

    let (url, _server) = serve_once("503 Service Unavailable", b"unavailable".to_vec());
    let error = request(&test_client(&url), &url).await.unwrap_err();
    assert_eq!(error, LoadError::Status(503));
    assert_eq!(error.to_string(), "Unable to load login servers (HTTP 503)");
}

#[tokio::test]
async fn rejects_oversized_and_invalid_server_lists() {
    let (url, _server) = serve_once("200 OK", vec![b' '; MAX_BODY + 1]);
    assert!(matches!(
        request(&test_client(&url), &url).await,
        Err(LoadError::Decode { status: 200, .. })
    ));
    let (url, _server) = serve_once("200 OK", br#"[{"sName":"incomplete"}]"#.to_vec());
    assert_eq!(
        request(&test_client(&url), &url).await,
        Err(LoadError::Payload)
    );
}

#[tokio::test]
async fn rejects_empty_bodies_as_payloads_and_cut_off_bodies_as_undecodable() {
    let (url, _server) = serve_once("200 OK", Vec::new());
    assert_eq!(
        request(&test_client(&url), &url).await,
        Err(LoadError::Payload)
    );
    let (url, _server) = serve_raw(
        b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 100\r\n\r\n[{\"sName\"".to_vec(),
    );
    assert!(matches!(
        request(&test_client(&url), &url).await,
        Err(LoadError::Decode { status: 200, .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn keeps_cached_servers_after_a_manual_refresh_fails() {
    let fake = Fake::new(|request| match request {
        1 => Ok(listing(json!([row("Artix", false)]))),
        _ => Err(LoadError::Status(503)),
    });
    let (servers, _) = serving(&fake);
    let first = servers.get().await.unwrap();
    assert_eq!(first.servers()[0].name(), "Artix");
    assert!(!first.servers()[0].online());
    assert_eq!(first.refresh_available_at(), None);
    advance(Duration::from_secs(15)).await;
    let refreshed = servers.refresh().await.unwrap();
    assert_eq!(refreshed.servers(), first.servers());
    assert_eq!(
        refreshed.refresh_available_at(),
        Some(UNIX_EPOCH + Duration::from_secs(30))
    );
    assert_eq!(servers.get().await.unwrap(), refreshed);
    assert_eq!(
        &*servers.pings().await.unwrap(),
        [ping("Artix", PingStatus::Offline)]
    );
    assert_eq!(servers.refresh().await.unwrap(), refreshed);
    assert_eq!(fake.requests(), 2);
}

#[tokio::test(start_paused = true)]
async fn reports_offline_success_invalid_refused_and_deadlines_in_input_order() {
    let fake = Fake::scripted(|_| unreachable!(), |_| replies([0, 10, 10, 10]));
    let mut invalid = row("Invalid", true);
    invalid["iPort"] = json!(-1);
    let targets = listing(json!([
        row("Offline", false),
        row("Connected", true),
        invalid,
        row("Refused", true),
        row("Timed out", true)
    ]));
    let started = tokio::time::Instant::now();
    let pending = tokio::spawn(ping_all(fake.clone(), targets));
    until(|| fake.dialed() == 3).await;
    advance(Duration::from_millis(25)).await;
    fake.connect(0);
    fake.refuse(1);
    assert_eq!(
        &*pending.await.unwrap(),
        [
            ping("Offline", PingStatus::Offline),
            ping("Connected", PingStatus::Ok { latency_ms: 10 }),
            ping("Invalid", PingStatus::Unreachable),
            ping("Refused", PingStatus::Unreachable),
            ping("Timed out", PingStatus::Timeout),
        ]
    );
    assert_eq!(started.elapsed(), Duration::from_secs(6));
    assert_eq!(fake.dialed(), 3);
    until(|| fake.open().is_empty()).await;
}

async fn ping_one(script: Vec<Step>) -> (PingStatus, Duration) {
    let fake = Fake::scripted(|_| unreachable!(), move |_| script.clone());
    let started = tokio::time::Instant::now();
    let pending = tokio::spawn(ping_all(fake.clone(), listing(json!([row("Artix", true)]))));
    until(|| fake.dialed() == 1).await;
    advance(Duration::from_millis(100)).await;
    fake.connect(0);
    let status = pending.await.unwrap()[0].status();
    let elapsed = started.elapsed();
    advance(Duration::from_secs(6)).await;
    until(|| fake.open().is_empty()).await;
    (status, elapsed)
}

#[tokio::test(start_paused = true)]
async fn reports_the_median_round_trip_after_the_connect_and_a_warm_up() {
    assert_eq!(
        ping_one(replies([400, 60, 75, 50])).await,
        (
            PingStatus::Ok { latency_ms: 60 },
            Duration::from_millis(685)
        )
    );
}

#[tokio::test(start_paused = true)]
async fn one_six_second_deadline_covers_the_connect_warm_up_and_samples() {
    assert_eq!(
        ping_one(replies([1300, 650, 650, 650])).await.0,
        PingStatus::Ok { latency_ms: 650 }
    );
    assert_eq!(
        ping_one(replies([5700, 60, 60, 60])).await.0,
        PingStatus::Ok { latency_ms: 60 }
    );
    assert_eq!(
        ping_one(replies([5750, 60, 60, 60])).await,
        (PingStatus::Timeout, Duration::from_secs(6))
    );
    assert_eq!(
        ping_one(Vec::new()).await,
        (PingStatus::Timeout, Duration::from_secs(6))
    );
}

#[tokio::test(start_paused = true)]
async fn reports_early_closes_and_unterminated_oversized_replies_as_unreachable() {
    for script in [
        vec![Step::Close],
        vec![
            Step::Reply(Duration::ZERO),
            Step::Reply(Duration::ZERO),
            Step::Close,
        ],
        vec![Step::Flood],
    ] {
        assert_eq!(ping_one(script).await.0, PingStatus::Unreachable);
    }
}

#[tokio::test(start_paused = true)]
async fn limits_concurrency_to_six_and_cancels_every_outstanding_connection() {
    let fake = Fake::new(|_| unreachable!());
    let rows = (0..20).map(|index| row(&index.to_string(), true)).collect();
    let pending = tokio::spawn(ping_all(fake.clone(), listing(Value::Array(rows))));
    until(|| fake.dialed() == 6).await;
    fake.connect(0);
    until(|| fake.dialed() == 7).await;
    assert_eq!(fake.open(), [1, 2, 3, 4, 5, 6]);
    pending.abort();
    let _ = pending.await;
    until(|| fake.open().is_empty()).await;
    advance(Duration::from_secs(10)).await;
    assert_eq!(fake.dialed(), 7);
}

#[tokio::test(start_paused = true)]
async fn shares_ping_loads_and_keeps_them_until_a_refresh_refetches_the_list() {
    let fake = Fake::scripted(
        |request| {
            Ok(listing(json!([
                row("Artix", request == 1),
                row("Yorumi", true)
            ])))
        },
        |_| replies([0, 10, 10, 10]),
    );
    let (servers, _) = serving(&fake);
    let (first, concurrent) = (servers.pings(), servers.pings());
    until(|| fake.dialed() == 2).await;
    assert_eq!(fake.requests(), 1);
    fake.connect(0);
    fake.connect(1);
    let (first, concurrent) = (first.await.unwrap(), concurrent.await.unwrap());
    assert_eq!(first, concurrent);
    assert_eq!(
        &*first,
        [
            ping("Artix", PingStatus::Ok { latency_ms: 10 }),
            ping("Yorumi", PingStatus::Ok { latency_ms: 10 }),
        ]
    );
    advance(Duration::from_secs(31)).await;
    assert_eq!(servers.pings().await.unwrap(), first);
    assert_eq!(fake.dialed(), 2);
    servers.refresh().await.unwrap();
    let refreshed = servers.pings();
    until(|| fake.dialed() == 3).await;
    assert_eq!(fake.requests(), 2);
    fake.connect(2);
    assert_eq!(
        &*refreshed.await.unwrap(),
        [
            ping("Artix", PingStatus::Offline),
            ping("Yorumi", PingStatus::Ok { latency_ms: 10 }),
        ]
    );
    servers.refresh().await.unwrap();
    assert_eq!(fake.requests(), 2);
}

#[tokio::test(start_paused = true)]
async fn pings_each_list_once_however_often_pings_are_asked_for() {
    let fake = Fake::new(|request| match request {
        1 | 2 => Ok(listing(json!([row("Artix", true)]))),
        _ => Err(LoadError::Status(503)),
    });
    let (servers, _) = serving(&fake);
    let mut dials = 0;
    for refetches in 1..=3 {
        let pending = servers.pings();
        until(|| fake.dialed() == dials + 1).await;
        fake.connect(dials);
        dials += 1;
        let measured = pending.await.unwrap();
        for _ in 0..29 {
            advance(Duration::from_secs(10)).await;
            assert_eq!(servers.pings().await.unwrap(), measured);
        }
        advance(Duration::from_millis(9_999)).await;
        assert_eq!(servers.pings().await.unwrap(), measured);
        assert_eq!((fake.requests(), fake.dialed()), (refetches, dials));
        advance(Duration::from_millis(1)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn keeps_six_sockets_across_overlapping_refreshes() {
    let fake = Fake::new(|_| {
        let rows = (0..12).map(|index| row(&format!("Server{index}"), true));
        Ok(listing(Value::Array(rows.collect())))
    });
    let (servers, _) = serving(&fake);
    let first = servers.pings();
    until(|| fake.dialed() == 6).await;
    servers.refresh().await.unwrap();
    let refreshed = servers.pings();
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert_eq!(fake.requests(), 2);
    assert_eq!(fake.open().len(), 6);
    for count in [6, 12, 18, 24] {
        until(|| fake.dialed() == count).await;
        let open = fake.open();
        assert_eq!(open.len(), 6);
        for index in open {
            fake.connect(index);
        }
    }
    assert_eq!(first.await.unwrap().len(), 12);
    assert_eq!(refreshed.await.unwrap().len(), 12);
    until(|| fake.open().is_empty()).await;
}

#[tokio::test(start_paused = true)]
async fn serves_the_cached_list_for_five_minutes() {
    let fake = Fake::new(|_| Ok(listing(json!([row("Artix", true)]))));
    let (servers, _) = serving(&fake);
    servers.get().await.unwrap();
    advance(Duration::from_millis(299_999)).await;
    servers.get().await.unwrap();
    assert_eq!(fake.requests(), 1);
    advance(Duration::from_millis(1)).await;
    servers.get().await.unwrap();
    assert_eq!(fake.requests(), 2);
}

#[tokio::test(start_paused = true)]
async fn ages_the_list_and_the_cooldown_across_a_system_sleep() {
    let fake = Fake::new(|_| Ok(listing(json!([row("Artix", true)]))));
    let (servers, wall) = serving(&fake);
    servers.refresh().await.unwrap();
    wall.sleep(Duration::from_secs(5 * 60));
    servers.get().await.unwrap();
    assert_eq!(fake.requests(), 2);
    let refreshed = servers.refresh().await.unwrap();
    assert_eq!(fake.requests(), 3);
    assert_eq!(
        refreshed.refresh_available_at(),
        Some(UNIX_EPOCH + Duration::from_secs(5 * 60 + 15))
    );
}

#[tokio::test(start_paused = true)]
async fn failed_loads_carry_when_a_refresh_is_next_available() {
    let fake = Fake::new(|_| Err(LoadError::Status(503)));
    let (servers, _) = serving(&fake);
    let unavailable = |refresh_available_at| AccountsError::Servers {
        error: LoadError::Status(503),
        refresh_available_at,
    };
    assert_eq!(servers.get().await, Err(unavailable(None)));
    advance(Duration::from_secs(1)).await;
    let cooling = Some(UNIX_EPOCH + Duration::from_secs(16));
    assert_eq!(servers.refresh().await, Err(unavailable(cooling)));
    assert_eq!(servers.refresh().await, Err(unavailable(cooling)));
    assert_eq!(servers.pings().await, Err(unavailable(cooling)));
    assert_eq!(fake.requests(), 4);
}
