use super::*;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::time::advance;

struct Fake {
    respond: Box<dyn Fn(usize) -> Result<Arc<Listing>, LoadError> + Send + Sync>,
    requests: AtomicUsize,
    sockets: Mutex<Vec<Socket>>,
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
        Arc::new(Self {
            respond: Box::new(respond),
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
}

impl Network for Fake {
    fn fetch(&self) -> BoxFuture<Result<Arc<Listing>, LoadError>> {
        let request = self.requests.fetch_add(1, Ordering::SeqCst) + 1;
        let outcome = (self.respond)(request);
        Box::pin(async move { outcome })
    }

    fn dial(&self, _host: &GameHost, _port: u16) -> BoxFuture<io::Result<()>> {
        let (connect, connected) = oneshot::channel();
        let closed = Arc::new(AtomicBool::new(false));
        self.sockets.lock().unwrap().push(Socket {
            connect: Some(connect),
            closed: closed.clone(),
        });
        let closes = Closes(closed);
        Box::pin(async move {
            let _closes = closes;
            connected
                .await
                .map_err(|_| io::Error::from(io::ErrorKind::ConnectionAborted))
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
async fn reports_offline_success_invalid_targets_and_deadlines_in_input_order() {
    let fake = Fake::new(|_| unreachable!());
    let mut invalid = row("Invalid", true);
    invalid["iPort"] = json!(-1);
    let targets = listing(json!([
        row("Offline", false),
        row("Connected", true),
        invalid,
        row("Timed out", true)
    ]));
    let pending = tokio::spawn(ping_all(fake.clone(), targets));
    until(|| fake.dialed() == 2).await;
    advance(Duration::from_millis(25)).await;
    fake.connect(0);
    until(|| fake.open() == [1]).await;
    advance(Duration::from_millis(1975)).await;
    assert_eq!(
        &*pending.await.unwrap(),
        [
            ping("Offline", PingStatus::Offline),
            ping("Connected", PingStatus::Ok { latency_ms: 25 }),
            ping("Invalid", PingStatus::Unreachable),
            ping("Timed out", PingStatus::Timeout),
        ]
    );
    assert_eq!(fake.dialed(), 2);
    assert!(fake.open().is_empty());
}

#[tokio::test(start_paused = true)]
async fn limits_concurrency_to_six_and_cancels_every_outstanding_connection() {
    let fake = Fake::new(|_| unreachable!());
    let rows = (0..20).map(|index| row(&index.to_string(), true)).collect();
    let pending = tokio::spawn(ping_all(fake.clone(), listing(Value::Array(rows))));
    until(|| fake.dialed() == 6).await;
    fake.connect(0);
    until(|| fake.dialed() == 7).await;
    pending.abort();
    let _ = pending.await;
    advance(Duration::from_secs(10)).await;
    assert_eq!(fake.dialed(), 7);
    assert!(fake.open().is_empty());
}

#[tokio::test(start_paused = true)]
async fn shares_ping_loads_expires_them_after_completion_and_invalidates_them_on_refresh() {
    let fake = Fake::new(|request| {
        Ok(listing(json!([
            row("Artix", request == 1),
            row("Yorumi", true)
        ])))
    });
    let (servers, _) = serving(&fake);
    let (first, concurrent) = (servers.pings(), servers.pings());
    until(|| fake.dialed() == 2).await;
    assert_eq!(fake.requests(), 1);
    advance(Duration::from_millis(10)).await;
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
    advance(Duration::from_millis(29_999)).await;
    assert_eq!(servers.pings().await.unwrap(), first);
    assert_eq!(fake.dialed(), 2);
    advance(Duration::from_millis(1)).await;
    let next = servers.pings();
    until(|| fake.dialed() == 4).await;
    fake.connect(2);
    fake.connect(3);
    next.await.unwrap();
    servers.refresh().await.unwrap();
    let refreshed = servers.pings();
    until(|| fake.dialed() == 5).await;
    assert_eq!(fake.requests(), 2);
    fake.connect(4);
    assert_eq!(
        refreshed.await.unwrap()[0],
        ping("Artix", PingStatus::Offline)
    );
    servers.refresh().await.unwrap();
    assert_eq!(fake.requests(), 2);
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
