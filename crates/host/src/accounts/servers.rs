use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use futures_util::{StreamExt, stream};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde::{Deserialize, Deserializer};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::Instant;
use url::Url;

use super::{AccountsError, Answer, Reply, reply};
use crate::net::GameHost;

const SERVERS_URL: &str = "https://game.aq.com/game/api/data/servers";
const REFRESH_COOLDOWN: Duration = Duration::from_secs(15);
const LIST_TTL: Duration = Duration::from_secs(5 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY: usize = 1024 * 1024;
const PING_TIMEOUT: Duration = Duration::from_secs(6);
const PING_CONCURRENCY: usize = 6;
const PING_SAMPLES: usize = 3;
const VERSION_CHECK: &[u8] =
    b"<msg t='sys'><body action='verChk' r='0'><ver v='157' /></body></msg>\0";
const MAX_REPLY: usize = 4 * 1024;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type Pings = Arc<[Ping]>;

#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    name: String,
    language: String,
    online: bool,
    upgrade: bool,
    player_count: f64,
    max_players: f64,
}

impl Server {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn online(&self) -> bool {
        self.online
    }

    pub fn upgrade(&self) -> bool {
        self.upgrade
    }

    pub fn player_count(&self) -> f64 {
        self.player_count
    }

    pub fn max_players(&self) -> f64 {
        self.max_players
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerList {
    listing: Arc<Listing>,
    refresh_available_at: Option<SystemTime>,
}

impl ServerList {
    pub fn servers(&self) -> &[Server] {
        &self.listing.servers
    }

    pub fn refresh_available_at(&self) -> Option<SystemTime> {
        self.refresh_available_at
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ping {
    server_name: String,
    status: PingStatus,
}

impl Ping {
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn status(&self) -> PingStatus {
        self.status
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PingStatus {
    Ok { latency_ms: u32 },
    Offline,
    Timeout,
    Unreachable,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    #[error("Unable to load login servers (HTTP {0})")]
    Status(u16),
    #[error("Invalid login servers payload")]
    Payload,
    #[error("Decode error ({status} GET {url})")]
    Decode { status: u16, url: String },
    #[error("Operation timed out after '10s'")]
    TimedOut,
    #[error("Transport error (GET {url})")]
    Transport { url: String },
}

#[derive(Clone)]
pub struct LoginServers {
    commands: mpsc::UnboundedSender<Command>,
}

impl LoginServers {
    pub(crate) fn start(runtime: &tokio::runtime::Handle) -> Self {
        Self::serving(runtime, Arc::new(Live::game()), SystemTime::now)
    }

    fn serving(
        runtime: &tokio::runtime::Handle,
        network: Arc<dyn Network>,
        clock: impl Fn() -> SystemTime + Send + 'static,
    ) -> Self {
        let (commands, received) = mpsc::unbounded_channel();
        runtime.spawn(serve(network, clock, received));
        Self { commands }
    }

    pub fn get(&self) -> Reply<ServerList> {
        self.ask(Command::Get)
    }

    pub fn refresh(&self) -> Reply<ServerList> {
        self.ask(Command::Refresh)
    }

    pub fn pings(&self) -> Reply<Pings> {
        self.ask(Command::Pings)
    }

    fn ask<T>(&self, command: impl FnOnce(Answer<T>) -> Command) -> Reply<T> {
        let (answer, reply) = reply();
        let _ = self.commands.send(command(answer));
        reply
    }
}

enum Command {
    Get(Answer<ServerList>),
    Refresh(Answer<ServerList>),
    Pings(Answer<Pings>),
}

trait Network: Send + Sync + 'static {
    fn fetch(&self) -> BoxFuture<Result<Arc<Listing>, LoadError>>;
    fn dial(&self, host: &GameHost, port: u16) -> BoxFuture<io::Result<Box<dyn Connection>>>;
}

trait Connection: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Connection for T {}

struct Live {
    url: Url,
    client: OnceLock<reqwest::Client>,
}

impl Live {
    fn game() -> Self {
        Self {
            url: Url::parse(SERVERS_URL).expect("the login servers URL parses"),
            client: OnceLock::new(),
        }
    }

    fn client(&self) -> Result<reqwest::Client, LoadError> {
        if let Some(client) = self.client.get() {
            return Ok(client.clone());
        }
        let client = client(reqwest::Client::builder()).map_err(|_| LoadError::Transport {
            url: self.url.to_string(),
        })?;
        Ok(self.client.get_or_init(|| client).clone())
    }
}

impl Network for Live {
    fn fetch(&self) -> BoxFuture<Result<Arc<Listing>, LoadError>> {
        let client = self.client();
        let url = self.url.clone();
        Box::pin(async move { request(&client?, &url).await.map(Arc::new) })
    }

    fn dial(&self, host: &GameHost, port: u16) -> BoxFuture<io::Result<Box<dyn Connection>>> {
        let host = host.clone();
        Box::pin(async move {
            let stream = crate::net::dial(&host, port).await?;
            Ok(Box::new(stream) as Box<dyn Connection>)
        })
    }
}

fn client(builder: reqwest::ClientBuilder) -> reqwest::Result<reqwest::Client> {
    builder.redirect(reqwest::redirect::Policy::none()).build()
}

async fn request(client: &reqwest::Client, url: &Url) -> Result<Listing, LoadError> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    crate::net::identify(url, &mut headers);
    let failed = |error: reqwest::Error| {
        if error.is_timeout() {
            LoadError::TimedOut
        } else {
            LoadError::Transport {
                url: url.to_string(),
            }
        }
    };
    let mut response = client
        .get(url.clone())
        .headers(headers)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .map_err(failed)?;
    let status = response.status();
    if !status.is_success() {
        return Err(LoadError::Status(status.as_u16()));
    }
    let undecodable = || LoadError::Decode {
        status: status.as_u16(),
        url: url.to_string(),
    };
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BODY as u64)
    {
        return Err(undecodable());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        if error.is_timeout() {
            LoadError::TimedOut
        } else {
            undecodable()
        }
    })? {
        if body.len() + chunk.len() > MAX_BODY {
            return Err(undecodable());
        }
        body.extend_from_slice(&chunk);
    }
    if body.is_empty() {
        return Err(LoadError::Payload);
    }
    Listing::parse(&body).map_err(|category| match category {
        serde_json::error::Category::Data => LoadError::Payload,
        _ => undecodable(),
    })
}

#[derive(Debug, PartialEq)]
struct Listing {
    servers: Vec<Server>,
    targets: Vec<Target>,
}

#[derive(Debug, PartialEq)]
enum Target {
    Offline,
    Unreachable,
    Dial(GameHost, u16),
}

#[derive(Deserialize)]
struct Row {
    #[serde(rename = "bOnline")]
    online: f64,
    #[serde(rename = "bUpg")]
    upgrade: f64,
    #[serde(rename = "iChat", default, deserialize_with = "number")]
    _chat: Option<f64>,
    #[serde(rename = "iCount")]
    player_count: f64,
    #[serde(rename = "iLevel", default, deserialize_with = "number")]
    _level: Option<f64>,
    #[serde(rename = "iMax")]
    max_players: f64,
    #[serde(rename = "iPort")]
    port: f64,
    #[serde(rename = "sIP")]
    ip: String,
    #[serde(rename = "sLang")]
    language: String,
    #[serde(rename = "sName")]
    name: String,
}

fn number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    f64::deserialize(deserializer).map(Some)
}

impl Listing {
    fn parse(body: &[u8]) -> Result<Self, serde_json::error::Category> {
        let rows: Vec<Row> = serde_json::from_slice(body).map_err(|error| error.classify())?;
        let (servers, targets) = rows
            .into_iter()
            .map(|row| {
                let target = if row.online != 1.0 {
                    Target::Offline
                } else {
                    let port = (row.port.fract() == 0.0 && (1.0..=65535.0).contains(&row.port))
                        .then_some(row.port as u16);
                    match (GameHost::parse(&row.ip), port) {
                        (Some(host), Some(port)) => Target::Dial(host, port),
                        _ => Target::Unreachable,
                    }
                };
                let server = Server {
                    name: row.name,
                    language: row.language,
                    online: row.online == 1.0,
                    upgrade: row.upgrade == 1.0,
                    player_count: row.player_count,
                    max_players: row.max_players,
                };
                (server, target)
            })
            .unzip();
        Ok(Self { servers, targets })
    }
}

async fn ping_all(network: Arc<dyn Network>, listing: Arc<Listing>) -> Pings {
    let mut statuses = vec![PingStatus::Offline; listing.targets.len()];
    let mut pinging = stream::iter(0..listing.targets.len())
        .map(|index| {
            let (network, listing) = (network.clone(), listing.clone());
            async move { (index, ping(&*network, &listing.targets[index]).await) }
        })
        .buffer_unordered(PING_CONCURRENCY);
    while let Some((index, status)) = pinging.next().await {
        statuses[index] = status;
    }
    listing
        .servers
        .iter()
        .zip(statuses)
        .map(|(server, status)| Ping {
            server_name: server.name.clone(),
            status,
        })
        .collect()
}

async fn ping(network: &dyn Network, target: &Target) -> PingStatus {
    let (host, port) = match target {
        Target::Offline => return PingStatus::Offline,
        Target::Unreachable => return PingStatus::Unreachable,
        Target::Dial(host, port) => (host, *port),
    };
    match tokio::time::timeout(PING_TIMEOUT, round_trip(network, host, port)).await {
        Ok(Ok(round_trip)) => PingStatus::Ok {
            latency_ms: (round_trip.as_secs_f64() * 1000.0).round() as u32,
        },
        Ok(Err(_)) => PingStatus::Unreachable,
        Err(_) => PingStatus::Timeout,
    }
}

async fn round_trip(network: &dyn Network, host: &GameHost, port: u16) -> io::Result<Duration> {
    let mut connection = network.dial(host, port).await?;
    exchange(&mut *connection).await?;
    let mut samples = [Duration::ZERO; PING_SAMPLES];
    for sample in &mut samples {
        let started = Instant::now();
        exchange(&mut *connection).await?;
        *sample = started.elapsed();
    }
    samples.sort();
    Ok(samples[PING_SAMPLES / 2])
}

async fn exchange(connection: &mut dyn Connection) -> io::Result<()> {
    connection.write_all(VERSION_CHECK).await?;
    let mut buffer = [0; 1024];
    let mut received = 0;
    loop {
        let read = connection.read(&mut buffer).await?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if buffer[..read].contains(&0) {
            return Ok(());
        }
        received += read;
        if received > MAX_REPLY {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
}

async fn serve(
    network: Arc<dyn Network>,
    clock: impl Fn() -> SystemTime,
    mut commands: mpsc::UnboundedReceiver<Command>,
) {
    let mut book = Book::default();
    let mut fetch: Option<BoxFuture<Result<Arc<Listing>, LoadError>>> = None;
    let mut wave: Option<BoxFuture<Pings>> = None;
    loop {
        tokio::select! {
            Some(command) = commands.recv() => book.command(clock(), command),
            outcome = finish(&mut fetch) => book.fetched(clock(), outcome),
            pings = finish(&mut wave) => book.pinged(pings),
        }
        if book.fetch.is_some() && fetch.is_none() {
            fetch = Some(network.fetch());
        }
        if let Some(running) = &book.wave
            && wave.is_none()
        {
            wave = Some(Box::pin(ping_all(network.clone(), running.listing.clone())));
        }
    }
}

async fn finish<T>(slot: &mut Option<BoxFuture<T>>) -> T {
    match slot {
        Some(work) => {
            let output = work.await;
            *slot = None;
            output
        }
        None => std::future::pending().await,
    }
}

#[derive(Default)]
struct Book {
    list: Option<Listed>,
    refreshed_at: Option<SystemTime>,
    fetch: Option<Fetch>,
    pings: Option<Pinged>,
    wave: Option<Wave>,
}

#[derive(Clone)]
struct Current {
    listing: Arc<Listing>,
    generation: u64,
}

struct Listed {
    current: Current,
    fetched_at: SystemTime,
}

struct Fetch {
    waiters: Vec<Waiter>,
    then: Vec<Waiter>,
}

struct Pinged {
    pings: Pings,
    generation: u64,
}

struct Wave {
    listing: Arc<Listing>,
    generation: u64,
    waiters: Vec<Answer<Pings>>,
    then: Vec<Answer<Pings>>,
}

enum Waiter {
    List(Answer<ServerList>),
    Pings(Answer<Pings>),
}

impl Waiter {
    fn fail(self, error: LoadError, refresh_available_at: Option<SystemTime>) {
        let error = AccountsError::Servers {
            error,
            refresh_available_at,
        };
        match self {
            Waiter::List(answer) => answer.send(Err(error)),
            Waiter::Pings(answer) => answer.send(Err(error)),
        }
    }
}

impl Book {
    fn command(&mut self, now: SystemTime, command: Command) {
        match command {
            Command::Get(answer) => self.load(now, Waiter::List(answer)),
            Command::Pings(answer) => self.load(now, Waiter::Pings(answer)),
            Command::Refresh(answer) => self.refresh(now, answer),
        }
    }

    fn refresh(&mut self, now: SystemTime, answer: Answer<ServerList>) {
        if self
            .refreshed_at
            .is_some_and(|at| elapsed(at, now) < REFRESH_COOLDOWN)
        {
            return self.load(now, Waiter::List(answer));
        }
        self.refreshed_at = Some(now);
        match &mut self.fetch {
            Some(fetch) => fetch.then.push(Waiter::List(answer)),
            None => self.start_fetch(Waiter::List(answer)),
        }
    }

    fn load(&mut self, now: SystemTime, waiter: Waiter) {
        if let Some(fetch) = &mut self.fetch {
            return if fetch.then.is_empty() {
                fetch.waiters.push(waiter)
            } else {
                fetch.then.push(waiter)
            };
        }
        match &self.list {
            Some(list) if elapsed(list.fetched_at, now) < LIST_TTL => {
                let current = list.current.clone();
                self.serve(&current, waiter)
            }
            _ => self.start_fetch(waiter),
        }
    }

    fn refresh_available_at(&self) -> Option<SystemTime> {
        self.refreshed_at.map(|at| at + REFRESH_COOLDOWN)
    }

    fn start_fetch(&mut self, waiter: Waiter) {
        self.fetch = Some(Fetch {
            waiters: vec![waiter],
            then: Vec::new(),
        });
    }

    fn fetched(&mut self, now: SystemTime, outcome: Result<Arc<Listing>, LoadError>) {
        let Some(Fetch { waiters, then }) = self.fetch.take() else {
            return;
        };
        if !then.is_empty() {
            self.fetch = Some(Fetch {
                waiters: then,
                then: Vec::new(),
            });
        }
        let generation = self
            .list
            .as_ref()
            .map_or(0, |list| list.current.generation + 1);
        let listing = match (outcome, &self.list) {
            (Ok(listing), _) => listing,
            (Err(error), Some(list)) => {
                tracing::warn!(
                    component = "accounts",
                    cached_server_count = list.current.listing.servers.len(),
                    %error,
                    "Failed to fetch servers; using cache"
                );
                list.current.listing.clone()
            }
            (Err(error), None) => {
                for waiter in waiters {
                    waiter.fail(error.clone(), self.refresh_available_at());
                }
                return;
            }
        };
        let current = Current {
            listing,
            generation,
        };
        self.list = Some(Listed {
            current: current.clone(),
            fetched_at: now,
        });
        for waiter in waiters {
            self.serve(&current, waiter);
        }
    }

    fn serve(&mut self, current: &Current, waiter: Waiter) {
        match waiter {
            Waiter::List(answer) => answer.send(Ok(ServerList {
                listing: current.listing.clone(),
                refresh_available_at: self.refresh_available_at(),
            })),
            Waiter::Pings(answer) => self.ping(current, answer),
        }
    }

    fn ping(&mut self, current: &Current, answer: Answer<Pings>) {
        if let Some(pinged) = &self.pings
            && pinged.generation == current.generation
        {
            return answer.send(Ok(pinged.pings.clone()));
        }
        match &mut self.wave {
            Some(wave) if wave.generation == current.generation => wave.waiters.push(answer),
            Some(wave) => wave.then.push(answer),
            None => {
                self.wave = Some(Wave {
                    listing: current.listing.clone(),
                    generation: current.generation,
                    waiters: vec![answer],
                    then: Vec::new(),
                })
            }
        }
    }

    fn pinged(&mut self, pings: Pings) {
        let Some(Wave {
            generation,
            waiters,
            then,
            ..
        }) = self.wave.take()
        else {
            return;
        };
        let Some(current) = self.list.as_ref().map(|list| list.current.clone()) else {
            return;
        };
        if generation == current.generation {
            self.pings = Some(Pinged {
                pings: pings.clone(),
                generation,
            });
        }
        for answer in waiters {
            answer.send(Ok(pings.clone()));
        }
        for answer in then {
            self.ping(&current, answer);
        }
    }
}

fn elapsed(since: SystemTime, now: SystemTime) -> Duration {
    now.duration_since(since).unwrap_or_default()
}

#[cfg(test)]
mod tests;
