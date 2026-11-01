mod io;
pub(crate) use io::Snapshot;
use io::{
    Capture, Observation, Observed, Selection, SelectionStatus, ServerList, Wait, loader_json,
    selection,
};

use crate::{
    ScriptLink,
    bridge::{Error, EventInterests, EventKind, Pending, Value},
    sessions::relogin::{Edit, Request, Shared},
};
use lucent_protocol::{
    auto_relogin::{AutoReloginPhase as Phase, AutoReloginRecords},
    command::Command,
};
use std::{
    sync::Arc,
    task::Waker,
    time::{Duration, Instant},
};

const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(500);

pub struct ReloginPolicy {
    link: ScriptLink,
    policy: Policy,
    waker: Waker,
}
impl ReloginPolicy {
    pub const INTERESTS: EventInterests =
        EventInterests::new(&[EventKind::Connection, EventKind::Invalidated]);
    pub fn new(link: ScriptLink, waker: Waker) -> Self {
        link.set_waker(waker.clone());
        link.records.relogin.set_waker(waker.clone());
        let policy = Policy::for_player(link.records.auto_relogin.read().clone(), Instant::now());
        Self {
            link,
            policy,
            waker,
        }
    }
    pub fn advance(&mut self, now: Instant) -> Option<Instant> {
        let mut draining = false;
        for index in 0..64 {
            match self.link.try_event() {
                Ok(Some(crate::bridge::ScriptEvent::Loader { event, .. }))
                    if event.name == "onConnection" =>
                {
                    if let Some(Value::String(status)) = event.args.first() {
                        self.policy.connection(status, now);
                    }
                }
                Err(Error::Overflow) | Ok(Some(crate::bridge::ScriptEvent::Invalidated)) => {
                    self.policy.resynchronize(now)
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => return None,
            }
            draining = index == 63;
        }
        let shared = self.link.records.relogin.clone();
        let at = self.policy.advance(&self.link, &shared, &self.waker, now);
        if draining { Some(now) } else { at }
    }
}

trait ReloginHost {
    fn snapshot(&self) -> Pending<Snapshot>;
    fn command(&self, command: Command) -> Pending<String>;
    fn call(&self, method: &'static str, args: Vec<Value>) -> Pending<Value>;
}
impl ReloginHost for ScriptLink {
    fn snapshot(&self) -> Pending<Snapshot> {
        self.relogin_snapshot()
    }
    fn command(&self, command: Command) -> Pending<String> {
        self.command(command)
    }
    fn call(&self, method: &'static str, args: Vec<Value>) -> Pending<Value> {
        self.call_loader(method, args)
    }
}
struct Setter {
    request: Request,
    work: SetterWork,
}
enum SetterWork {
    Immediate,
    Enable(Box<Enable>),
    Server(Box<ServerList>),
    Retry(Box<Observation>),
}
struct Enable {
    capture: Option<Capture>,
    observation: Observation,
}
impl Setter {
    fn answer(&mut self, answer: Result<String, Error>) {
        if let Some(reply) = self.request.reply.take() {
            let _ = reply.send(answer);
        }
    }
}
#[derive(Default)]
enum RecoveryState {
    #[default]
    Idle,
    AwaitingSession,
    WaitingCredentials(Instant),
    Delay(Instant),
    Attempt {
        resume: bool,
    },
    Kick(Wait<Value>),
    KickDuration(Wait<Value>),
    WaitingKick {
        until: Instant,
        next: Instant,
        check: Option<Wait<Value>>,
    },
    Login(Wait<String>),
    Outcome(Instant),
    Manual(Instant),
    Select {
        reply: Wait<Value>,
        expires: Instant,
    },
    SelectionResult {
        value: Option<Selection>,
        expires: Instant,
    },
    SelectAgain {
        at: Instant,
        expires: Instant,
    },
    Player(Instant),
    PlayerCheck,
    Logout {
        reply: Wait<String>,
        cause: LogoutCause,
    },
    Backoff {
        at: Instant,
        delay: Duration,
    },
    Failed,
}
enum LogoutCause {
    ServerFull(String),
    PlayerUnready,
}
impl LogoutCause {
    fn detail(&self) -> &str {
        match self {
            Self::ServerFull(detail) => detail,
            Self::PlayerUnready => "Player did not become ready",
        }
    }
}
#[derive(Default)]
struct Recovery {
    state: RecoveryState,
    attempt: u8,
    fresh_login: bool,
}
impl Recovery {
    fn phase(&self) -> Phase {
        match &self.state {
            RecoveryState::Idle => Phase::Armed,
            RecoveryState::AwaitingSession | RecoveryState::WaitingCredentials(_) => {
                Phase::AwaitingSession
            }
            RecoveryState::Delay(_) | RecoveryState::Attempt { .. } => Phase::WaitingDelay,
            RecoveryState::Kick(_) if self.attempt > 0 => Phase::Backoff {
                delay_ms: (5_u64 << (self.attempt - 1)) as f64 * 1000.0,
            },
            RecoveryState::Kick(_) => Phase::WaitingDelay,
            RecoveryState::KickDuration(_) | RecoveryState::WaitingKick { .. } => {
                Phase::WaitingKick
            }
            RecoveryState::Login(_) => Phase::LoggingIn,
            RecoveryState::Outcome(_) | RecoveryState::Manual(_) => Phase::AwaitingServerSelect,
            RecoveryState::Select { .. }
            | RecoveryState::SelectionResult { .. }
            | RecoveryState::SelectAgain { .. } => Phase::SelectingServer,
            RecoveryState::Player(_) | RecoveryState::PlayerCheck => Phase::AwaitingPlayerReady,
            RecoveryState::Logout {
                cause: LogoutCause::ServerFull(_),
                ..
            } => Phase::SelectingServer,
            RecoveryState::Logout {
                cause: LogoutCause::PlayerUnready,
                ..
            } => Phase::AwaitingPlayerReady,
            RecoveryState::Backoff { delay, .. } => Phase::Backoff {
                delay_ms: delay.as_secs_f64() * 1000.0,
            },
            RecoveryState::Failed => Phase::Failed,
        }
    }
}
#[derive(Default)]
struct Credentials {
    work: CaptureWork,
    failing: bool,
}
impl Credentials {
    fn schedule(&mut self, at: Instant) {
        if !matches!(self.work, CaptureWork::Running { .. }) {
            self.work = CaptureWork::Scheduled(at);
        }
    }
}
#[derive(Default)]
enum CaptureWork {
    #[default]
    Idle,
    Scheduled(Instant),
    Running {
        capture: Box<Capture>,
        deadline: Instant,
    },
}
#[derive(Default)]
struct Observer {
    work: ObservationWork,
    watch_until: Option<Instant>,
}
#[derive(Default)]
enum ObservationWork {
    #[default]
    Idle,
    Scheduled {
        at: Instant,
        resynchronize: bool,
    },
    Running {
        observation: Box<Observation>,
        resynchronize: bool,
    },
}
impl Observer {
    fn schedule(&mut self, at: Instant) {
        if !matches!(self.work, ObservationWork::Running { .. }) {
            self.work = ObservationWork::Scheduled {
                at,
                resynchronize: false,
            };
        }
    }
    fn start(&mut self, host: &impl ReloginHost, waker: &Waker, now: Instant) {
        self.work = ObservationWork::Running {
            observation: Box::new(Observation::new(host, waker, now)),
            resynchronize: false,
        };
    }
    fn deadline(&self) -> Option<Instant> {
        match &self.work {
            ObservationWork::Idle => None,
            ObservationWork::Scheduled { at, .. } => Some(*at),
            ObservationWork::Running { observation, .. } => Some(observation.deadline),
        }
    }
}
#[derive(Default)]
struct Policy {
    records: AutoReloginRecords,
    setter: Option<Setter>,
    credentials: Credentials,
    observer: Observer,
    observed: Observed,
    recovery: Recovery,
}
impl Policy {
    fn for_player(records: AutoReloginRecords, now: Instant) -> Self {
        let attempt = records
            .status
            .attempts_remaining
            .map_or(0, |left| 3 - (left as u8).min(3));
        let recovery = if !records.config.enabled {
            RecoveryState::Idle
        } else {
            match records.status.phase {
                Phase::Failed => RecoveryState::Failed,
                Phase::AwaitingSession => RecoveryState::AwaitingSession,
                _ if !records.status.captured => RecoveryState::Idle,
                Phase::Armed => RecoveryState::Idle,
                Phase::WaitingDelay => RecoveryState::Delay(now),
                Phase::Backoff { delay_ms } => RecoveryState::Backoff {
                    at: now + Duration::from_secs_f64(delay_ms / 1000.0),
                    delay: Duration::from_secs_f64(delay_ms / 1000.0),
                },
                _ => RecoveryState::Attempt { resume: false },
            }
        };
        let observer = if !matches!(
            recovery,
            RecoveryState::Idle | RecoveryState::AwaitingSession | RecoveryState::Failed
        ) {
            Observer {
                work: ObservationWork::Scheduled {
                    at: now,
                    resynchronize: false,
                },
                watch_until: None,
            }
        } else {
            Observer::default()
        };
        Self {
            records,
            recovery: Recovery {
                state: recovery,
                attempt,
                fresh_login: false,
            },
            observer,
            ..Self::default()
        }
    }
    fn publish(&mut self, shared: &Shared) {
        self.records.status.phase = self.recovery.phase();
        shared.changes.set(&shared.records, self.records.clone());
    }
    fn resynchronize(&mut self, now: Instant) {
        if self.records.config.enabled {
            self.observer.work = ObservationWork::Scheduled {
                at: now,
                resynchronize: true,
            };
        }
    }
    fn connection(&mut self, status: &str, now: Instant) {
        if !self.records.config.enabled {
            return;
        }
        match status {
            "OnConnectionLost" | "OnConnectionFailed"
                if matches!(
                    self.recovery.state,
                    RecoveryState::Idle | RecoveryState::AwaitingSession
                ) =>
            {
                self.credentials.failing = false;
                self.recovery.state = if self.records.status.captured {
                    RecoveryState::Delay(now)
                } else {
                    self.credentials.schedule(now);
                    RecoveryState::WaitingCredentials(now)
                };
                self.records.status.attempts_remaining = None;
                self.records.status.last_error = None;
                self.observed = Observed::default();
                self.observer.work = ObservationWork::Idle;
                self.observer.schedule(now);
            }
            "OnConnection" => {
                self.observer.watch_until = Some(now + Duration::from_secs(10));
                self.observer.schedule(now);
                if !self.records.status.captured {
                    self.credentials.schedule(now);
                }
            }
            _ => {}
        }
    }
    fn armed(&mut self) {
        self.recovery.fresh_login = false;
        self.records.status.attempts_remaining = None;
        self.records.status.last_error = None;
        self.recovery.state = RecoveryState::Idle;
        self.observer = Observer::default();
    }
    fn fail(&mut self, detail: &str, retryable: bool, now: Instant) {
        self.records.status.last_error = Some(sentence(detail));
        if retryable && self.recovery.attempt < 3 {
            let delay = 5_u64 << self.recovery.attempt;
            self.recovery.state = RecoveryState::Backoff {
                at: now + Duration::from_secs(delay),
                delay: Duration::from_secs(delay),
            };
        } else {
            self.records.status.attempts_remaining = retryable.then_some(0.0);
            self.recovery.state = RecoveryState::Failed;
            self.observer.work = ObservationWork::Idle;
        }
    }
    fn captured(
        &mut self,
        result: Result<(String, String, bool), Error>,
        shared: &Shared,
    ) -> Result<(), Error> {
        let (username, password, ready) = result?;
        if !self.records.status.captured {
            if username.is_empty() || password.is_empty() {
                if !matches!(self.recovery.state, RecoveryState::WaitingCredentials(_)) {
                    self.recovery.state = if ready {
                        RecoveryState::Idle
                    } else {
                        RecoveryState::AwaitingSession
                    };
                }
            } else {
                self.records.status.captured = true;
                self.records.status.last_error = None;
                self.records.status.attempts_remaining = None;
                self.records.status.username = Some(username.clone());
                *shared.credentials.lock() = Some((username, password));
                if let RecoveryState::WaitingCredentials(start) = self.recovery.state {
                    self.recovery.state = RecoveryState::Delay(start);
                } else {
                    self.recovery.state = RecoveryState::Idle;
                }
            }
        }
        Ok(())
    }
    fn recovery_deadline(&self, now: Instant) -> Option<Instant> {
        match &self.recovery.state {
            RecoveryState::Idle
            | RecoveryState::AwaitingSession
            | RecoveryState::Failed
            | RecoveryState::WaitingCredentials(_)
            | RecoveryState::PlayerCheck
            | RecoveryState::SelectionResult { .. } => None,
            RecoveryState::Delay(start) => {
                Some(*start + Duration::from_secs_f64(self.records.config.delay_ms / 1000.0))
            }
            RecoveryState::Attempt { .. } => Some(now),
            RecoveryState::Kick(wait) | RecoveryState::KickDuration(wait) => Some(wait.deadline),
            RecoveryState::Login(wait) => Some(wait.deadline),
            RecoveryState::Outcome(at) | RecoveryState::Manual(at) | RecoveryState::Player(at) => {
                Some(*at)
            }
            RecoveryState::Backoff { at, .. } => Some(*at),
            RecoveryState::Select { reply, expires } => Some(reply.deadline.min(*expires)),
            RecoveryState::SelectAgain { at, .. } => Some(*at),
            RecoveryState::Logout { reply, .. } => Some(reply.deadline),
            RecoveryState::WaitingKick { until, next, check } => Some(
                check
                    .as_ref()
                    .map_or((*until).min(*next), |reply| reply.deadline.min(*until)),
            ),
        }
    }
    fn advance(
        &mut self,
        host: &impl ReloginHost,
        shared: &Arc<Shared>,
        waker: &Waker,
        now: Instant,
    ) -> Option<Instant> {
        let deadline = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.transition(host, shared, waker, now)
        })) {
            Ok(at) => at,
            Err(panic) => {
                tracing::error!("auto relogin transition panicked: {}", panic_message(panic));
                self.fail("Auto relogin failed unexpectedly.", false, now);
                self.credentials.work = CaptureWork::Idle;
                if self.records.config.enabled && !self.records.status.captured {
                    self.credentials.schedule(now + REPLY_TIMEOUT);
                }
                if let Some(mut setter) = self.setter.take() {
                    setter.answer(Err(Error::ReadPanicked));
                }
                Some(now)
            }
        };
        self.publish(shared);
        deadline
    }
    fn transition(
        &mut self,
        host: &impl ReloginHost,
        shared: &Arc<Shared>,
        waker: &Waker,
        now: Instant,
    ) -> Option<Instant> {
        if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.setters(host, shared, waker, now)
        })) {
            tracing::warn!("auto relogin setter panicked: {}", panic_message(panic));
            if let Some(mut setter) = self.setter.take() {
                setter.answer(Err(Error::ReadPanicked));
            }
            if self.records.config.enabled && !self.records.status.captured {
                self.credentials.schedule(now + REPLY_TIMEOUT);
            }
        }
        if self.records.config.enabled {
            if !self.records.status.captured && matches!(self.credentials.work, CaptureWork::Idle) {
                self.credentials.schedule(now);
            }
            if matches!(self.credentials.work, CaptureWork::Scheduled(at) if at <= now)
                && self.setter.is_none()
            {
                self.credentials.work = CaptureWork::Running {
                    capture: Box::new(Capture::new(host, waker, now)),
                    deadline: now + REPLY_TIMEOUT,
                };
            }
            if let CaptureWork::Running { capture, .. } = &mut self.credentials.work
                && let Some(result) = capture.poll(now)
            {
                self.credentials.work = CaptureWork::Idle;
                match self.captured(result, shared) {
                    Ok(()) => self.credentials.failing = false,
                    Err(error) => {
                        if !self.credentials.failing {
                            tracing::warn!("auto relogin capture failed: {error}");
                        }
                        self.credentials.failing = true;
                    }
                }
                if !self.records.status.captured {
                    self.credentials.schedule(now + REPLY_TIMEOUT);
                }
            }
            if let ObservationWork::Scheduled { at, resynchronize } = self.observer.work
                && at <= now
            {
                self.observer.work = ObservationWork::Running {
                    observation: Box::new(Observation::new(host, waker, now)),
                    resynchronize,
                };
            }
            if let ObservationWork::Running {
                observation,
                resynchronize,
            } = &mut self.observer.work
                && let Some(observed) = observation.poll(now)
            {
                let resynchronize = *resynchronize;
                self.observer.work = ObservationWork::Idle;
                self.observed = observed;
                if self.observed.ready {
                    self.armed();
                } else {
                    if resynchronize {
                        match self.observed.connected {
                            Some(false) => self.connection("OnConnectionLost", now),
                            Some(true) => self.connection("OnConnection", now),
                            None => {}
                        }
                    }
                    if !matches!(
                        self.recovery.state,
                        RecoveryState::Idle
                            | RecoveryState::AwaitingSession
                            | RecoveryState::Failed
                    ) || self.observer.watch_until.is_some_and(|at| at > now)
                    {
                        self.observer.schedule(now + POLL);
                    } else {
                        self.observer.watch_until = None;
                    }
                }
            }
            for _ in 0..16 {
                if !self.recover(host, shared, waker, now) {
                    break;
                }
            }
        }
        let setter_at = self.setter.as_ref().map(|s| {
            let deadline = match &s.work {
                SetterWork::Immediate => now,
                SetterWork::Enable(enable) => enable.observation.deadline,
                SetterWork::Retry(observation) => observation.deadline,
                SetterWork::Server(list) => list.deadline,
            };
            if s.request.reply.is_some() {
                deadline.min(s.request.expires)
            } else {
                deadline
            }
        });
        [
            (self.setter.is_none() && shared.has_pending()).then_some(now),
            self.recovery_deadline(now),
            self.observer.deadline(),
            match &self.credentials.work {
                CaptureWork::Idle => None,
                CaptureWork::Scheduled(at) => self.setter.is_none().then_some(*at),
                CaptureWork::Running { deadline, .. } => Some(*deadline),
            },
            setter_at,
        ]
        .into_iter()
        .flatten()
        .min()
    }
    fn setters(
        &mut self,
        host: &impl ReloginHost,
        shared: &Arc<Shared>,
        waker: &Waker,
        now: Instant,
    ) {
        for _ in 0..64 {
            if self.setter.is_none() {
                while let Some(mut request) = shared.take() {
                    if !request.alive(now) {
                        if let Some(reply) = request.reply.take() {
                            let _ = reply.send(Err(Error::Timeout));
                        }
                        continue;
                    }
                    let work = match &request.edit {
                        Edit::Enabled(false) => {
                            self.armed();
                            self.records.config.enabled = false;
                            self.records.status.captured = false;
                            self.records.status.username = None;
                            *shared.credentials.lock() = None;
                            self.credentials.work = CaptureWork::Idle;
                            SetterWork::Immediate
                        }
                        Edit::Enabled(true) => {
                            self.records.config.enabled = true;
                            SetterWork::Enable(Box::new(Enable {
                                capture: (!self.records.status.captured)
                                    .then(|| Capture::new(host, waker, now)),
                                observation: Observation::new(host, waker, now),
                            }))
                        }
                        Edit::Delay(delay) => {
                            self.records.config.delay_ms = normalize_delay(*delay);
                            SetterWork::Immediate
                        }
                        Edit::Server(server)
                            if server.as_ref().is_some_and(|s| !s.trim().is_empty()) =>
                        {
                            SetterWork::Server(Box::new(ServerList::new(host, waker, now)))
                        }
                        Edit::Server(server) => {
                            if self.server(server.clone()) {
                                SetterWork::Retry(Box::new(Observation::new(host, waker, now)))
                            } else {
                                SetterWork::Immediate
                            }
                        }
                    };
                    self.setter = Some(Setter { request, work });
                    break;
                }
            }
            let Some(mut setter) = self.setter.take() else {
                break;
            };
            if now >= setter.request.expires {
                setter.answer(Err(Error::Timeout));
            }
            let mut next_work = None;
            let result = match &mut setter.work {
                SetterWork::Immediate => Some(Ok(())),
                SetterWork::Enable(enable) => {
                    let captured = if let Some(capture) = &mut enable.capture {
                        capture
                            .poll(now)
                            .map(|result| self.captured(result, shared))
                    } else {
                        Some(Ok(()))
                    };
                    match captured {
                        Some(Ok(())) => {
                            enable.capture = None;
                            enable.observation.poll(now).map(|observed| {
                                if observed.ready {
                                    self.armed();
                                }
                                self.observed = observed;
                                Ok(())
                            })
                        }
                        other => other,
                    }
                }
                SetterWork::Retry(observation) => observation.poll(now).map(|observed| {
                    if observed.ready {
                        self.armed();
                    } else {
                        self.recovery.attempt = 0;
                        self.recovery.fresh_login = false;
                        self.recovery.state = RecoveryState::Attempt { resume: true };
                        self.observer.schedule(now);
                    }
                    self.observed = observed;
                    Ok(())
                }),
                SetterWork::Server(list) => match list.poll(now) {
                    Some(Ok(names)) => {
                        let Edit::Server(server) = &setter.request.edit else {
                            unreachable!()
                        };
                        let requested = server.as_deref().unwrap().trim();
                        let canonical = names
                            .iter()
                            .find(|s| s.to_lowercase() == requested.to_lowercase())
                            .map_or(requested, |s| s.as_str())
                            .to_owned();
                        if self.server(Some(canonical)) {
                            next_work = Some(SetterWork::Retry(Box::new(Observation::new(
                                host, waker, now,
                            ))));
                            None
                        } else {
                            Some(Ok(()))
                        }
                    }
                    Some(Err(error)) => Some(Err(error)),
                    None => None,
                },
            };
            if let Some(work) = next_work {
                setter.work = work;
            }
            if let Some(result) = result {
                if self.records.config.enabled && !self.records.status.captured {
                    self.credentials.schedule(now + REPLY_TIMEOUT);
                }
                self.publish(shared);
                setter.answer(result.map(|()| serde_json::to_string(&self.records).unwrap()));
            } else {
                self.setter = Some(setter);
                break;
            }
        }
    }
    fn server(&mut self, server: Option<String>) -> bool {
        let server = server.map(|s| s.trim().to_owned());
        if self.records.config.server != server {
            self.records.config.server = server;
            self.records.status.last_error = None;
            if self.records.config.enabled
                && self.records.status.captured
                && matches!(self.recovery.state, RecoveryState::Failed)
            {
                self.records.status.attempts_remaining = None;
                return true;
            }
        }
        false
    }
    fn recover(
        &mut self,
        host: &impl ReloginHost,
        shared: &Shared,
        waker: &Waker,
        now: Instant,
    ) -> bool {
        let state = std::mem::take(&mut self.recovery.state);
        self.recovery.state = match state {
            RecoveryState::Delay(start)
                if now
                    >= start + Duration::from_secs_f64(self.records.config.delay_ms / 1000.0) =>
            {
                self.recovery.attempt = 0;
                RecoveryState::Attempt { resume: false }
            }
            RecoveryState::Attempt { resume } => {
                self.records.status.attempts_remaining = Some(f64::from(3 - self.recovery.attempt));
                self.records.status.last_error = None;
                if resume && self.observed.select {
                    RecoveryState::Manual(now + Duration::from_millis(250))
                } else {
                    RecoveryState::Kick(Wait::new(
                        host.call("auth.isTemporarilyKicked", vec![]),
                        waker,
                        now,
                    ))
                }
            }
            RecoveryState::Kick(reply) => match reply.poll(now) {
                Some(Ok(Value::Bool(true))) => RecoveryState::KickDuration(Wait::new(
                    host.call(
                        "flash.getGameObject",
                        vec![Value::String("mcLogin.warning.n".into())],
                    ),
                    waker,
                    now,
                )),
                Some(_) => {
                    return self.login(host, shared, waker, now);
                }
                None => {
                    self.recovery.state = RecoveryState::Kick(reply);
                    return false;
                }
            },
            RecoveryState::KickDuration(reply) => match reply.poll(now) {
                Some(result) => {
                    let json = result.ok().and_then(loader_json);
                    let seconds = json
                        .and_then(|v| v.as_f64())
                        .filter(|n| n.is_finite() && *n > 0.0)
                        .map_or(60.0, |n| n.ceil().min(70.0));
                    RecoveryState::WaitingKick {
                        until: now + Duration::from_secs(70),
                        next: now + Duration::from_secs_f64((seconds + 1.0).min(70.0)),
                        check: None,
                    }
                }
                None => {
                    self.recovery.state = RecoveryState::KickDuration(reply);
                    return false;
                }
            },
            RecoveryState::WaitingKick {
                until,
                next,
                mut check,
            } => {
                if now >= next && check.is_none() {
                    check = Some(Wait::new(
                        host.call("auth.isTemporarilyKicked", vec![]),
                        waker,
                        now,
                    ));
                }
                if let Some(result) = check.as_ref().and_then(|r| r.poll(now)) {
                    if !matches!(result, Ok(Value::Bool(true))) {
                        return self.login(host, shared, waker, now);
                    }
                    check = None;
                    if now >= until {
                        self.fail("Login could not be submitted", true, now);
                        return true;
                    }
                    self.recovery.state = RecoveryState::WaitingKick {
                        until,
                        next: (now + Duration::from_secs(5)).min(until),
                        check,
                    };
                    return false;
                }
                if now >= until {
                    self.fail("Login could not be submitted", true, now);
                    return true;
                }
                self.recovery.state = RecoveryState::WaitingKick { until, next, check };
                return false;
            }
            RecoveryState::Login(reply) => match reply.poll(now) {
                Some(Ok(json)) if json == "true" => {
                    RecoveryState::Outcome(now + Duration::from_secs(15))
                }
                Some(_) => {
                    self.fail("Login could not be submitted", true, now);
                    return true;
                }
                None => {
                    self.recovery.state = RecoveryState::Login(reply);
                    return false;
                }
            },
            RecoveryState::Outcome(expires) => {
                if self.observed.logged_in {
                    RecoveryState::Player(now + Duration::from_secs(10))
                } else if self.observed.select {
                    self.recovery.fresh_login = false;
                    RecoveryState::Manual(now + Duration::from_millis(250))
                } else if now >= expires {
                    self.fail("Login did not reach server selection", true, now);
                    return true;
                } else {
                    self.recovery.state = RecoveryState::Outcome(expires);
                    return false;
                }
            }
            RecoveryState::Manual(at) if now >= at => {
                if self.observed.logged_in {
                    RecoveryState::Player(now + Duration::from_secs(10))
                } else if let Some(server) = &self.records.config.server {
                    RecoveryState::Select {
                        reply: Wait::new(
                            host.call("auth.connectTo", vec![Value::String(server.clone())]),
                            waker,
                            now,
                        ),
                        expires: now + Duration::from_secs(5),
                    }
                } else {
                    self.armed();
                    return false;
                }
            }
            RecoveryState::Select { reply, expires } => match reply.poll(now) {
                Some(result) => {
                    self.observer.start(host, waker, now);
                    RecoveryState::SelectionResult {
                        value: result.ok().and_then(selection),
                        expires,
                    }
                }
                None => {
                    self.recovery.state = RecoveryState::Select { reply, expires };
                    return false;
                }
            },
            RecoveryState::SelectionResult { value, expires }
                if !matches!(self.observer.work, ObservationWork::Running { .. }) =>
            {
                if self.observed.logged_in
                    || value
                        .as_ref()
                        .is_some_and(|s| s.status == SelectionStatus::Selected)
                {
                    RecoveryState::Player(now + Duration::from_secs(10))
                } else if value
                    .as_ref()
                    .is_none_or(|s| s.status == SelectionStatus::NotReady)
                    && now < expires
                {
                    RecoveryState::SelectAgain {
                        at: now + POLL,
                        expires,
                    }
                } else {
                    let detail = value
                        .as_ref()
                        .map_or("Server selection is not ready", |s| s.message.as_str());
                    if value
                        .as_ref()
                        .is_some_and(|s| s.reason.as_deref() == Some("full"))
                    {
                        self.recovery.fresh_login = true;
                        RecoveryState::Logout {
                            reply: Wait::new(host.command(Command::Logout {}), waker, now),
                            cause: LogoutCause::ServerFull(detail.to_owned()),
                        }
                    } else {
                        self.fail(
                            detail,
                            value
                                .as_ref()
                                .is_none_or(|s| s.status == SelectionStatus::NotReady),
                            now,
                        );
                        return true;
                    }
                }
            }
            RecoveryState::SelectAgain { at, expires } if now >= at => {
                if let Some(server) = &self.records.config.server {
                    RecoveryState::Select {
                        reply: Wait::new(
                            host.call("auth.connectTo", vec![Value::String(server.clone())]),
                            waker,
                            now,
                        ),
                        expires,
                    }
                } else {
                    self.armed();
                    return false;
                }
            }
            RecoveryState::Player(expires) if now >= expires => {
                self.observer.start(host, waker, now);
                RecoveryState::PlayerCheck
            }
            RecoveryState::PlayerCheck
                if !matches!(self.observer.work, ObservationWork::Running { .. }) =>
            {
                RecoveryState::Logout {
                    reply: Wait::new(host.command(Command::Logout {}), waker, now),
                    cause: LogoutCause::PlayerUnready,
                }
            }
            RecoveryState::Logout { reply, cause } => match reply.poll(now) {
                Some(result) => {
                    self.fail(
                        if matches!(result, Ok(ref json) if json == "null") {
                            cause.detail()
                        } else {
                            "Logout could not be submitted"
                        },
                        true,
                        now,
                    );
                    return true;
                }
                None => {
                    self.recovery.state = RecoveryState::Logout { reply, cause };
                    return false;
                }
            },
            RecoveryState::Backoff { at, .. } if now >= at => {
                self.recovery.attempt += 1;
                RecoveryState::Attempt {
                    resume: self.observed.select && !self.recovery.fresh_login,
                }
            }
            other => {
                self.recovery.state = other;
                return false;
            }
        };
        true
    }
    fn login(
        &mut self,
        host: &impl ReloginHost,
        shared: &Shared,
        waker: &Waker,
        now: Instant,
    ) -> bool {
        let Some((username, password)) = shared.credentials.lock().clone() else {
            self.fail("Username and password are required", false, now);
            return false;
        };

        self.recovery.state = RecoveryState::Login(Wait::new(
            host.command(Command::Login { username, password }),
            waker,
            now,
        ));
        true
    }
}
fn normalize_delay(delay: f64) -> f64 {
    if delay.is_finite() {
        delay.clamp(0.0, 300_000.0)
    } else {
        3000.0
    }
}
fn sentence(detail: &str) -> String {
    let detail = detail.trim();
    if detail.is_empty() {
        return "Auto relogin failed.".into();
    }
    let mut chars = detail.chars();
    let mut text = chars.next().unwrap().to_uppercase().to_string();
    text.push_str(chars.as_str());
    if !text.ends_with(['.', '!', '?']) {
        text.push('.');
    }
    text
}
fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
