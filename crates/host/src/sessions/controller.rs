use super::{
    lifecycle::{self as lc, Lifecycle, RunId},
    reaper::{self, Reapers},
};
use crate::scripting::{Kill, RuntimeError, ScriptRun};
use crate::{
    HostError, ScriptLink, Session, Surface,
    automation::{AutoZonePolicy, ReloginPolicy, SettingsPolicy},
    bridge,
};
use lucent_protocol::script::{Script, ScriptOutcome, ScriptPhase, ScriptStatus};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    task::{Wake, Waker},
    time::{Duration, Instant},
};
use tracing::{Level, info, warn};

pub struct ScriptRequest {
    script: Script,
    inbox: mpsc::Sender<Msg>,
    messages: mpsc::Receiver<Msg>,
}
#[derive(Clone)]
pub struct ScriptControls {
    inbox: mpsc::Sender<Msg>,
    run: RunId,
}
impl ScriptRequest {
    pub fn new(script: Script) -> (Self, ScriptControls) {
        let (inbox, messages) = mpsc::channel();
        let controls = ScriptControls {
            inbox: inbox.clone(),
            run: RunId(1),
        };
        (
            Self {
                script,
                inbox,
                messages,
            },
            controls,
        )
    }
}
impl ScriptControls {
    pub fn stop(&self) {
        let _ = self
            .inbox
            .send(Msg::Event(lc::Event::Command(lc::Command::Stop {
                run: self.run,
                reason: String::new(),
            })));
    }
    pub fn kill(&self) {
        let _ = self
            .inbox
            .send(Msg::Event(lc::Event::Command(lc::Command::Kill {
                run: self.run,
            })));
    }
    pub fn status(&self) {
        let _ = self.inbox.send(Msg::Status);
    }
}
pub struct Supervisor {
    inbox: mpsc::Sender<Msg>,
    active: Arc<parking_lot::RwLock<lc::Status>>,
    shutdown: Arc<Shutdown>,
}
pub(super) struct Shutdown {
    inbox: mpsc::Sender<Msg>,
    thread: parking_lot::Mutex<Option<reaper::Join>>,
    reapers: Arc<Reapers>,
    session: u64,
    dispatch: tracing::Dispatch,
}
impl Shutdown {
    pub(super) fn stop(&self) {
        let mut thread = self.thread.lock();
        let Some(thread) = thread.take() else { return };
        let _guard = tracing::dispatcher::set_default(&self.dispatch);
        let deadline = Instant::now() + reaper::SHUTDOWN_TIMEOUT;
        let _ = self.inbox.send(Msg::Shutdown);
        if !reaper::join_until(thread, deadline) {
            warn!(
                "session {}: abandoning session controller at shutdown deadline",
                self.session
            );
        }
        self.reapers.stop_session(self.session, deadline);
    }
}
enum Msg {
    Player(Box<Player>),
    Wake,
    Start(Box<(Script, mpsc::Sender<Result<RunId, RuntimeError>>)>),
    Queue(lc::QueueCommand, mpsc::Sender<Result<(), RuntimeError>>),
    Event(lc::Event),
    RunReady(RunId),
    RunExited(RunId),
    Status,
    Shutdown,
}
struct Player {
    watch: ScriptLink,
    relogin: ScriptLink,
    auto_zone: ScriptLink,
    jobs: ScriptLink,
}
impl Supervisor {
    pub(super) fn controls(&self) -> Option<ScriptControls> {
        let status = self.active.read();
        let run = status
            .active
            .as_ref()
            .map(|active| active.run)
            .or_else(|| {
                status
                    .waiting_to_restart
                    .then(|| status.latest.as_ref().unwrap().run)
            })?;
        Some(ScriptControls {
            inbox: self.inbox.clone(),
            run,
        })
    }
    pub fn start(
        session: &mut Session,
        script: Option<ScriptRequest>,
    ) -> Result<Self, RuntimeError> {
        let (inbox, messages, script) = match script {
            Some(ScriptRequest {
                script,
                inbox,
                messages,
            }) => (inbox, messages, Some(script)),
            None => {
                let (inbox, messages) = mpsc::channel();
                (inbox, messages, None)
            }
        };
        let first = attach(session).ok_or(RuntimeError::Stopped)?;
        let binding = lc::Binding {
            player: lc::PlayerId(1),
            connection: lc::ConnectionEpoch(0),
        };
        let machine = Lifecycle::new(binding);
        let active = session.lifecycle_status.clone();
        let reapers = session.host.shared.reapers.clone();
        let reaper = reapers.start(session.id).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                RuntimeError::AlreadySupervised
            } else {
                error.into()
            }
        })?;
        let supervision = Supervision {
            reaper,
            status: session.script_status.clone(),
            active: active.clone(),
            changes: session.changes.clone(),
            session: first.watch.id(),
            notify: Arc::new(Notify {
                inbox: inbox.clone(),
                queued: AtomicBool::new(false),
            }),
            machine,
            worker: None,
            watch: None,
            readiness: lc::Readiness::default(),
            binding,
            connected: None,
            username: String::new(),
            ready_read: None,
            auth_reads: Vec::new(),
            auth_retry: 0,
            logout: None,
            safe_move: None,
            clock: Instant::now(),
            deadlines: BTreeSet::new(),
            settings: None,
            relogin: None,
            auto_zone: None,
            jobs: None,
            deadline: None,
        };
        session
            .changes
            .watch(Waker::from(supervision.notify.clone()));
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let worker_dispatch = dispatch.clone();
        let thread = reaper::Join::spawn(
            format!("lucent-supervisor-{}", first.watch.id()),
            move || {
                let _dispatch = tracing::dispatcher::set_default(&worker_dispatch);
                supervision.run(first, script, messages);
            },
        );
        let thread = match thread {
            Ok(thread) => thread,
            Err(error) => {
                reapers.stop_session(session.id, Instant::now() + reaper::SHUTDOWN_TIMEOUT);
                return Err(error.into());
            }
        };
        let shutdown = Arc::new(Shutdown {
            inbox: inbox.clone(),
            thread: parking_lot::Mutex::new(Some(thread)),
            reapers,
            session: session.id,
            dispatch,
        });
        session.controller = Some(shutdown.clone());
        Ok(Self {
            inbox,
            active,
            shutdown,
        })
    }
    pub fn start_script(
        &mut self,
        _session: &mut Session,
        script: Script,
    ) -> Result<ScriptControls, RuntimeError> {
        let (reply, answer) = mpsc::channel();
        self.inbox
            .send(Msg::Start(Box::new((script, reply))))
            .map_err(|_| RuntimeError::Stopped)?;
        let run = answer.recv().map_err(|_| RuntimeError::Stopped)??;
        Ok(ScriptControls {
            inbox: self.inbox.clone(),
            run,
        })
    }
    pub fn script_queue(&self, command: lc::QueueCommand) -> Result<(), RuntimeError> {
        let (reply, answer) = mpsc::channel();
        self.inbox
            .send(Msg::Queue(command, reply))
            .map_err(|_| RuntimeError::Stopped)?;
        answer.recv().map_err(|_| RuntimeError::Stopped)?
    }

    pub fn restart_player(
        &mut self,
        session: &mut Session,
        surface: Surface,
    ) -> Result<(), HostError> {
        session.restart(surface)?;
        if let Some(player) = attach(session) {
            let _ = self.inbox.send(Msg::Player(Box::new(player)));
        }
        Ok(())
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown.stop();
    }
}
fn attach(session: &mut Session) -> Option<Player> {
    Some(Player {
        watch: session.attach(bridge::EventInterests::new(&[
            bridge::EventKind::Connection,
            bridge::EventKind::Invalidated,
            bridge::EventKind::Login,
            bridge::EventKind::Logout,
            bridge::EventKind::JoinMap,
            bridge::EventKind::PlayersChanged,
        ]))?,
        relogin: session.attach(ReloginPolicy::INTERESTS)?,
        auto_zone: session.attach(AutoZonePolicy::INTERESTS)?,
        jobs: session.attach(crate::game::jobs::ProfileJobs::INTERESTS)?,
    })
}
struct Notify {
    inbox: mpsc::Sender<Msg>,
    queued: AtomicBool,
}
impl Wake for Notify {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        if !self.queued.swap(true, Ordering::AcqRel) {
            let _ = self.inbox.send(Msg::Wake);
        }
    }
}
struct Worker {
    id: RunId,
    run: ScriptRun,
    revoke: Option<Box<dyn FnOnce() + Send>>,
}
struct Reaped {
    inbox: mpsc::Sender<Msg>,
    run: RunId,
}
impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.inbox.send(Msg::Event(lc::Event::Reaped(self.run)));
    }
}
struct ReadyRead {
    run: RunId,
    binding: lc::Binding,
    deadline: u64,
    world: bridge::PendingCall,
    player: bridge::PendingCall,
    world_ready: Option<bool>,
    player_ready: Option<bool>,
}
struct Logout {
    run: RunId,
    pending: bridge::Pending<String>,
    deadline: Instant,
    guard: Arc<crate::game::jobs::safe_move::Guard>,
}
impl Drop for Logout {
    fn drop(&mut self) {
        self.guard.close();
    }
}
struct SafeMove {
    run: RunId,
    phase: lc::SafePhase,
    job: crate::game::jobs::safe_move::SafeMoveJob,
}
struct Supervision {
    reaper: reaper::Sender,
    status: Arc<parking_lot::RwLock<ScriptStatus>>,
    active: Arc<parking_lot::RwLock<lc::Status>>,
    changes: Arc<super::Changes>,
    session: u64,
    notify: Arc<Notify>,
    machine: Lifecycle,
    worker: Option<Worker>,
    watch: Option<ScriptLink>,
    readiness: lc::Readiness,
    binding: lc::Binding,
    connected: Option<bool>,
    username: String,
    ready_read: Option<ReadyRead>,
    auth_reads: Vec<bridge::Pending<String>>,
    auth_retry: u64,
    logout: Option<Logout>,
    safe_move: Option<SafeMove>,
    clock: Instant,
    deadlines: BTreeSet<u64>,
    settings: Option<SettingsPolicy>,
    relogin: Option<ReloginPolicy>,
    auto_zone: Option<AutoZonePolicy>,
    jobs: Option<crate::game::jobs::ProfileJobs>,
    deadline: Option<Instant>,
}
struct RunNotify {
    inbox: mpsc::Sender<Msg>,
    id: RunId,
    ready: bool,
}
impl Wake for RunNotify {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.inbox.send(if self.ready {
            Msg::RunReady(self.id)
        } else {
            Msg::RunExited(self.id)
        });
    }
}
impl Supervision {
    fn now(&self) -> u64 {
        self.clock
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
    fn options(&self) -> lc::Options {
        use lucent_protocol::script_options::RoomPolicy;
        let options = self.watch.as_ref().unwrap().records.script_options();
        lc::Options {
            room_policy: match options.room_policy {
                RoomPolicy::Public => lc::RoomPolicy::Public,
                RoomPolicy::RandomPrivate => lc::RoomPolicy::RandomPrivate,
                RoomPolicy::Specific { room_number } => lc::RoomPolicy::Specific(room_number),
            },
            safe_start_stop: options.safe_start_stop,
            restart_after_reconnect: options.restart_after_reconnect,
        }
    }
    fn transition(&mut self, event: lc::Event) -> lc::Transition {
        self.machine
            .advance(event, &self.readiness, &self.options(), self.now())
    }
    fn advance(&mut self, event: lc::Event) {
        let result = self.transition(event);
        self.effects(result.effects);
    }
    fn request(
        &mut self,
        script: Script,
        reply: Option<mpsc::Sender<Result<RunId, RuntimeError>>>,
    ) {
        let name = script.entry().to_string();
        let result = self.transition(lc::Event::Command(lc::Command::Start(lc::Request {
            name: name.clone(),
            reference: name,
            script: Arc::new(script),
            inputs: Default::default(),
        })));
        let answer = if result.rejection.is_some() {
            Err(RuntimeError::Busy)
        } else {
            Ok(self.machine.status().active.as_ref().unwrap().run)
        };
        self.effects(result.effects);
        if let Some(reply) = reply {
            let _ = reply.send(answer);
        }
    }
    fn run(mut self, first: Player, script: Option<Script>, messages: mpsc::Receiver<Msg>) {
        self.player(first);
        if let Some(script) = script {
            self.request(script, None);
        }
        loop {
            self.settle();
            let message = match self.deadline {
                Some(at) => messages.recv_timeout(at.saturating_duration_since(Instant::now())),
                None => messages
                    .recv()
                    .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
            };
            match message {
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) | Ok(Msg::Shutdown) => break,
                Ok(Msg::Player(player)) => self.player(*player),
                Ok(Msg::Wake) => {
                    self.notify.queued.store(false, Ordering::Release);
                    self.advance(lc::Event::OptionsChanged);
                }
                Ok(Msg::Start(request)) => {
                    let (script, reply) = *request;
                    self.request(script, Some(reply));
                }
                Ok(Msg::Queue(command, reply)) => {
                    let result = self.transition(lc::Event::Command(lc::Command::Queue(command)));
                    let answer = match result.rejection {
                        None => Ok(()),
                        Some(lc::Rejection::Busy) => Err(RuntimeError::Busy),
                        Some(error) => {
                            Err(RuntimeError::JavaScript(format!("script queue: {error:?}")))
                        }
                    };
                    self.effects(result.effects);
                    let _ = reply.send(answer);
                }
                Ok(Msg::Event(event)) => self.advance(event),
                Ok(Msg::RunReady(id)) => {
                    if self
                        .worker
                        .as_ref()
                        .is_some_and(|w| w.id == id && w.run.take_ready())
                    {
                        self.advance(lc::Event::RunReady(id));
                    }
                }
                Ok(Msg::RunExited(id)) => {
                    if let Some(worker) = &self.worker
                        && worker.id == id
                    {
                        let outcome = outcome(worker.run.wait(Duration::ZERO));
                        self.advance(lc::Event::RunExited { run: id, outcome });
                    }
                }
                Ok(Msg::Status) => self.print_status(),
            }
        }
        let mut binding = self.binding;
        binding.player.0 += 1;
        self.advance(lc::Event::PlayerReplaced(binding));
        if let Some(worker) = self.worker.take() {
            worker.run.kill();
            if let Some(revoke) = worker.revoke {
                revoke();
            }
            self.reaper.reap(move || drop(worker.run));
        }
    }
    fn player(&mut self, player: Player) {
        let replacing = self.watch.is_some();
        self.ready_read = None;
        self.logout = None;
        self.auth_reads.clear();
        self.auth_retry = 0;
        self.connected = None;
        self.username.clear();
        self.readiness = lc::Readiness::default();
        if replacing {
            self.binding.player.0 += 1;
            self.binding.connection.0 = 0;
            self.advance(lc::Event::PlayerReplaced(self.binding));
        }
        let Player {
            watch,
            relogin,
            auto_zone,
            jobs,
        } = player;
        watch.set_waker(Waker::from(self.notify.clone()));
        self.watch = Some(watch);
        info!("session {}: supervisor started", self.session);
        self.jobs = None;
        self.jobs = Some(crate::game::jobs::ProfileJobs::new(
            jobs,
            Waker::from(self.notify.clone()),
        ));
        self.settings = Some(SettingsPolicy::new(
            &relogin,
            Waker::from(self.notify.clone()),
        ));
        self.auto_zone = Some(AutoZonePolicy::new(
            auto_zone,
            Waker::from(self.notify.clone()),
        ));
        self.relogin = Some(ReloginPolicy::new(
            relogin,
            Waker::from(self.notify.clone()),
        ));
    }
    fn settle(&mut self) {
        self.deadline = self
            .settings
            .as_mut()
            .and_then(|p| p.advance(Instant::now()));
        for at in [
            self.relogin
                .as_mut()
                .and_then(|p| p.advance(Instant::now())),
            self.auto_zone
                .as_mut()
                .and_then(|p| p.advance(Instant::now())),
            self.jobs.as_mut().and_then(|p| p.advance(Instant::now())),
        ]
        .into_iter()
        .flatten()
        {
            self.schedule(at);
        }
        self.projection();
        if let Some(safe_move) = &mut self.safe_move {
            if let Some(outcome) = safe_move
                .job
                .advance(Instant::now(), &Waker::from(self.notify.clone()))
            {
                let run = safe_move.run;
                let phase = safe_move.phase;
                if matches!(
                    outcome,
                    crate::game::safe_move::Outcome::Error(_)
                        | crate::game::safe_move::Outcome::TimedOut
                        | crate::game::safe_move::Outcome::Unavailable
                ) {
                    warn!("session {}: safe move failed: {outcome:?}", self.session);
                }
                self.safe_move = None;
                self.advance(lc::Event::SafeMoveDone { run, phase });
            } else if let Some(at) = safe_move.job.deadline() {
                self.schedule(at);
            }
        }
        self.completions();
        let now = self.now();
        if self.deadlines.first().is_some_and(|at| *at <= now) {
            self.deadlines.retain(|at| *at > now);
            self.advance(lc::Event::Deadline);
        }
        if let Some(&at) = self.deadlines.first() {
            self.schedule(self.clock + Duration::from_millis(at));
        }
    }
    fn schedule(&mut self, at: Instant) {
        self.deadline = Some(self.deadline.map_or(at, |old| old.min(at)));
    }
    fn projection(&mut self) {
        let Some(watch) = &self.watch else {
            return;
        };
        let mut connections = Vec::new();
        let mut draining = false;
        let mut stopped = false;
        for index in 0..64 {
            match watch.try_event() {
                Ok(Some(bridge::ScriptEvent::Loader { event, .. }))
                    if event.name == "onConnection" =>
                {
                    if let Some(lucent_protocol::bridge::Value::String(status)) = event.args.first()
                    {
                        match status.as_str() {
                            "OnConnection" => connections.push(true),
                            "OnConnectionLost" | "OnConnectionFailed" => connections.push(false),
                            _ => {}
                        }
                    }
                }
                Ok(Some(_)) | Err(bridge::Error::Overflow) => {}
                Ok(None) => break,
                Err(_) => {
                    stopped = true;
                    break;
                }
            }
            draining = index == 63;
        }
        if stopped {
            if self.machine.status().active.is_some() {
                let mut binding = self.binding;
                binding.player.0 += 1;
                self.advance(lc::Event::PlayerReplaced(binding));
            }
            return;
        }
        let snapshot = watch.read_for(Duration::ZERO, |p| {
            (
                lc::Readiness {
                    logged_in: p.auth.logged_in,
                    username: p.auth.username.clone(),
                    projections_ready: p.current_readiness().missing.is_empty(),
                },
                p.connected,
            )
        });
        let Ok((readiness, connected)) = snapshot else {
            self.schedule(Instant::now() + Duration::from_millis(1));
            return;
        };
        self.readiness = readiness;
        if !self.readiness.username.trim().is_empty() {
            let name = self.readiness.username.trim().to_lowercase();
            if !self.username.is_empty() && self.username != name {
                self.advance(lc::Event::AccountChanged);
            }
            self.username = name;
        }
        if connections.is_empty() && connected != self.connected {
            connections.extend(connected);
        }
        for connected in connections {
            self.connected = Some(connected);
            if connected {
                self.binding.connection.0 += 1;
                self.advance(lc::Event::ConnectionRestored(self.binding));
            } else {
                self.advance(lc::Event::ConnectionLost(self.binding));
            }
        }
        self.advance(lc::Event::ProjectionChanged);
        let status = self.machine.status();
        if self.readiness.projections_ready
            && (status.active.is_some() || status.waiting_to_restart)
            && (!self.readiness.logged_in || self.readiness.username.trim().is_empty())
            && self.auth_reads.is_empty()
            && self.now() >= self.auth_retry
        {
            let watch = self.watch.as_ref().unwrap();
            for command in [
                lucent_protocol::command::Command::RefreshCredentials {},
                lucent_protocol::command::Command::RefreshLoggedIn {},
            ] {
                let pending = watch.command(command);
                pending.set_waker(Waker::from(self.notify.clone()));
                self.auth_reads.push(pending);
            }
            self.auth_retry = self.now() + 100;
            self.deadlines.insert(self.auth_retry);
        }
        if draining {
            self.schedule(Instant::now());
        }
    }
    fn completions(&mut self) {
        let now = self.now();
        self.auth_reads
            .retain(|pending| pending.try_recv().is_none());
        if let Some(read) = &mut self.ready_read {
            if read.world_ready.is_none() {
                read.world_ready = read
                    .world
                    .try_recv()
                    .map(|r| matches!(r, Ok(lucent_protocol::bridge::Value::Bool(true))));
            }
            if read.player_ready.is_none() {
                read.player_ready = read
                    .player
                    .try_recv()
                    .map(|r| matches!(r, Ok(lucent_protocol::bridge::Value::Bool(true))));
            }
            if read.binding != self.binding {
                let run = read.run;
                self.ready_read = None;
                self.advance(lc::Event::PlayerReady { run, ready: false });
            } else if now >= read.deadline {
                self.ready_read = None;
            } else if let (Some(world), Some(player)) = (read.world_ready, read.player_ready) {
                let id = read.run;
                self.ready_read = None;
                self.advance(lc::Event::PlayerReady {
                    run: id,
                    ready: world && player,
                });
            }
        }
        if let Some(logout) = &self.logout {
            if let Some(result) = logout.pending.try_recv().or_else(|| {
                (Instant::now() >= logout.deadline).then_some(Err(bridge::Error::Timeout))
            }) {
                let id = logout.run;
                if let Err(error) = result {
                    warn!("session {}: logout failed: {error}", self.session);
                }
                self.logout = None;
                self.advance(lc::Event::LogoutDone(id));
            } else {
                self.schedule(logout.deadline);
            }
        }
    }

    fn effects(&mut self, effects: Vec<lc::Effect>) {
        let mut effects = VecDeque::from(effects);
        while let Some(effect) = effects.pop_front() {
            let mut answer = None;
            match effect {
                lc::Effect::Publish(status) => self.publish(status),
                lc::Effect::ScheduleDeadline(at) => {
                    self.deadlines.insert(at);
                }
                lc::Effect::ReadPlayerReady {
                    run,
                    binding,
                    deadline,
                } => {
                    if binding == self.binding
                        && let Some(watch) = &self.watch
                    {
                        let world = watch.call_loader("world.isLoaded", vec![]);
                        let player = watch.call_loader("player.isLoaded", vec![]);
                        world.set_waker(Waker::from(self.notify.clone()));
                        player.set_waker(Waker::from(self.notify.clone()));
                        self.ready_read = Some(ReadyRead {
                            run,
                            binding,
                            deadline,
                            world,
                            player,
                            world_ready: None,
                            player_ready: None,
                        });
                    }
                }
                lc::Effect::Spawn {
                    run,
                    binding,
                    request,
                } => {
                    let result = if binding != self.binding {
                        Err(RuntimeError::Stopped)
                    } else {
                        let link = self
                            .watch
                            .as_ref()
                            .unwrap()
                            .fork(bridge::EventInterests::ALL);
                        let revoke = Box::new(link.revoker());
                        let notify = |ready| {
                            Waker::from(Arc::new(RunNotify {
                                inbox: self.notify.inbox.clone(),
                                id: run,
                                ready,
                            }))
                        };
                        ScriptRun::spawn(
                            link,
                            (*request.script).clone(),
                            request.inputs,
                            notify(true),
                            notify(false),
                        )
                        .map(|worker| Worker {
                            id: run,
                            run: worker,
                            revoke: Some(revoke),
                        })
                    };
                    match result {
                        Ok(worker) => self.worker = Some(worker),
                        Err(error) => {
                            answer = Some(lc::Event::RunExited {
                                run,
                                outcome: outcome(Err(error)),
                            })
                        }
                    }
                }
                lc::Effect::SignalStop { run } => {
                    if let Some(worker) = &self.worker
                        && worker.id == run
                    {
                        worker.run.stop_script();
                        info!(
                            "session {}: stopping {}",
                            self.session,
                            self.machine.status().active.as_ref().unwrap().name
                        );
                    }
                }
                lc::Effect::Kill { run } => {
                    if let Some(worker) = &self.worker
                        && worker.id == run
                    {
                        worker.run.kill();
                        info!(
                            "session {}: killing {}",
                            self.session,
                            self.machine.status().active.as_ref().unwrap().name
                        );
                    }
                }
                lc::Effect::Revoke { run } => {
                    if self.ready_read.as_ref().is_some_and(|read| read.run == run) {
                        self.ready_read = None;
                    }
                    if let Some(worker) = &mut self.worker
                        && worker.id == run
                        && let Some(revoke) = worker.revoke.take()
                    {
                        revoke();
                    }
                }
                lc::Effect::Reap { run } => {
                    if self.worker.as_ref().is_some_and(|worker| worker.id == run) {
                        let worker = self.worker.take().unwrap();
                        let inbox = self.notify.inbox.clone();
                        self.reaper.reap(move || {
                            let _reaped = Reaped { inbox, run };
                            drop(worker);
                        });
                    } else {
                        answer = Some(lc::Event::Reaped(run));
                    }
                }
                lc::Effect::SafeMove {
                    run,
                    binding,
                    phase,
                    room_policy,
                } => {
                    if binding == self.binding
                        && let Some(watch) = &self.watch
                    {
                        let policy = match room_policy {
                            lc::RoomPolicy::Public => crate::game::safe_move::RoomPolicy::Public,
                            lc::RoomPolicy::Specific(room) => {
                                crate::game::safe_move::RoomPolicy::Specific { room }
                            }
                            lc::RoomPolicy::RandomPrivate => {
                                crate::game::safe_move::RoomPolicy::RandomPrivate {
                                    room: random_private_room(),
                                }
                            }
                        };
                        self.safe_move = Some(SafeMove {
                            run,
                            phase,
                            job: crate::game::jobs::safe_move::SafeMoveJob::new(
                                watch,
                                policy,
                                &self.readiness.username,
                                Instant::now(),
                            ),
                        });
                        self.schedule(Instant::now());
                    } else {
                        answer = Some(lc::Event::SafeMoveDone { run, phase });
                    }
                }
                lc::Effect::CancelSafeMove { run, phase } => {
                    if let Some(safe_move) = &mut self.safe_move
                        && safe_move.run == run
                        && safe_move.phase == phase
                    {
                        safe_move.job.cancel();
                        self.schedule(Instant::now());
                    } else {
                        answer = Some(lc::Event::SafeMoveDone { run, phase });
                    }
                }
                lc::Effect::Logout { run, binding } => {
                    if binding == self.binding
                        && let Some(watch) = &self.watch
                    {
                        let guard = crate::game::jobs::safe_move::Guard::new(
                            watch,
                            &self.readiness.username,
                        );
                        let deadline = Instant::now() + Duration::from_secs(5);
                        let (reply, pending) = bridge::Pending::channel();
                        pending.set_waker(Waker::from(self.notify.clone()));
                        watch.inbox().post(crate::player::Msg::LifecycleLogout {
                            guard: guard.clone(),
                            deadline,
                            reply,
                        });
                        self.logout = Some(Logout {
                            run,
                            pending,
                            deadline,
                            guard,
                        });
                        self.schedule(deadline);
                    } else {
                        answer = Some(lc::Event::LogoutDone(run));
                    }
                }
                lc::Effect::Terminal(terminal) => {
                    let (level, line) = terminal_line(&terminal);
                    if level == Level::WARN {
                        warn!("session {}: {line}", self.session);
                    } else {
                        info!("session {}: {line}", self.session);
                    }
                }
                lc::Effect::Prepare { id, entry } => {
                    answer = Some(lc::Event::Prepared {
                        id,
                        result: Ok(entry),
                    })
                }
            }
            if let Some(event) = answer {
                let transition = self.transition(event);
                effects.extend(transition.effects);
            }
        }
    }
    fn publish(&self, status: lc::Status) {
        let old = self.active.read().clone();
        let phase = match status.active.as_ref().map(|a| a.phase) {
            None if status.waiting_to_restart => ScriptPhase::Waiting,
            None => ScriptPhase::Idle,
            Some(lc::RunPhase::Waiting) => ScriptPhase::Waiting,
            Some(lc::RunPhase::SafeBefore) => ScriptPhase::Preparing,
            Some(lc::RunPhase::Booting) => ScriptPhase::Starting,
            Some(lc::RunPhase::Running) => ScriptPhase::Running,
            Some(lc::RunPhase::Stopping) => ScriptPhase::Stopping,
            Some(lc::RunPhase::Finishing) => ScriptPhase::Finishing,
        };
        if old.active.as_ref().map(|a| (&a.run, a.phase))
            != status.active.as_ref().map(|a| (&a.run, a.phase))
        {
            if let Some(active) = &status.active {
                match active.phase {
                    lc::RunPhase::Waiting => info!(
                        "session {}: {} is waiting for login",
                        self.session, active.name
                    ),
                    lc::RunPhase::Running => {
                        info!("session {}: {} started", self.session, active.name)
                    }
                    _ => {}
                }
            } else {
                tracing::debug!("session {}: script worker reaped", self.session);
            }
        }
        let next = ScriptStatus {
            phase,
            script: status.active.as_ref().map(|a| a.name.clone()).or_else(|| {
                status
                    .waiting_to_restart
                    .then(|| status.latest.as_ref().unwrap().name.clone())
            }),
            last_end: status.latest.as_ref().map(|t| terminal_line(t).1),
        };
        let unchanged = *self.status.read() == next;
        *self.active.write() = status.clone();
        self.changes.set(&self.status, next);
        if old != status && unchanged {
            self.changes.notify();
        }
    }
    fn print_status(&self) {
        match self.machine.status().active {
            Some(active) => {
                let phase = match active.phase {
                    lc::RunPhase::Waiting => "waiting for login",
                    lc::RunPhase::Running => "running",
                    lc::RunPhase::Stopping => "stopping",
                    lc::RunPhase::Finishing => "finishing",
                    _ => "booting",
                };
                info!("session {}: {} is {phase}", self.session, active.name);
            }
            None => info!("session {}: no script is running", self.session),
        }
    }
}
fn outcome(result: Result<ScriptOutcome, RuntimeError>) -> lc::Outcome {
    match result {
        Ok(ScriptOutcome::Completed) => lc::Outcome::Completed,
        Ok(ScriptOutcome::Failed { message, details }) => lc::Outcome::Failed(match details {
            Some(details) => format!("{message}\n{details}"),
            None => message,
        }),
        Ok(ScriptOutcome::StoppedByScript { reason }) => {
            lc::Outcome::Stopped(lc::StopReason::Script(reason))
        }
        Ok(ScriptOutcome::Stopped { reason }) => {
            lc::Outcome::Stopped(lc::StopReason::Requested(reason.unwrap_or_default()))
        }
        Ok(ScriptOutcome::Exited {
            logout,
            close_client,
        }) => lc::Outcome::Exited {
            logout,
            close_client,
        },
        Err(RuntimeError::Killed(Kill::HeapLimit)) => lc::Outcome::Killed(lc::KillReason::Heap),
        Err(RuntimeError::Killed(Kill::Requested)) => {
            lc::Outcome::Killed(lc::KillReason::Requested)
        }
        Err(RuntimeError::Killed(_)) => lc::Outcome::Killed(lc::KillReason::Watchdog),
        Err(error) => lc::Outcome::Failed(error.to_string()),
    }
}
fn terminal_line(terminal: &lc::Terminal) -> (Level, String) {
    let result = match &terminal.outcome {
        lc::Outcome::Completed => Ok(ScriptOutcome::Completed),
        lc::Outcome::Failed(message) => Ok(ScriptOutcome::Failed {
            message: message.clone(),
            details: None,
        }),
        lc::Outcome::Stopped(lc::StopReason::Script(reason)) => {
            Ok(ScriptOutcome::StoppedByScript {
                reason: reason.clone(),
            })
        }
        lc::Outcome::Stopped(lc::StopReason::PlayerReplaced) => Err(RuntimeError::Stopped),
        lc::Outcome::Stopped(reason) => Ok(ScriptOutcome::Stopped {
            reason: if reason.message().is_empty() {
                None
            } else {
                Some(reason.message().into())
            },
        }),
        lc::Outcome::Exited { close_client, .. } => Ok(ScriptOutcome::Exited {
            logout: false,
            close_client: *close_client,
        }),
        lc::Outcome::Killed(reason) => Err(RuntimeError::Killed(match reason {
            lc::KillReason::Requested => Kill::Requested,
            lc::KillReason::Watchdog => Kill::Watchdog,
            lc::KillReason::Heap => Kill::HeapLimit,
        })),
    };
    ended(&terminal.name, result)
}
fn ended(name: &str, result: Result<ScriptOutcome, RuntimeError>) -> (Level, String) {
    let (level, end) = match result {
        Ok(ScriptOutcome::Completed) => (Level::INFO, "completed".into()),
        Ok(ScriptOutcome::Failed { message, details }) => (
            Level::WARN,
            match details {
                Some(details) => format!("failed: {message}\n{details}"),
                None => format!("failed: {message}"),
            },
        ),
        Ok(ScriptOutcome::StoppedByScript { reason }) => {
            (Level::INFO, with_reason("stopped itself", reason))
        }
        Ok(ScriptOutcome::Exited {
            close_client: false,
            ..
        }) => (Level::INFO, "exited".into()),
        Ok(ScriptOutcome::Exited {
            close_client: true, ..
        }) => (
            Level::INFO,
            "exited and asked to close the game client".into(),
        ),
        Ok(ScriptOutcome::Stopped { reason }) => (Level::INFO, with_reason("was stopped", reason)),
        Err(RuntimeError::Killed(Kill::Requested)) => (Level::WARN, "was killed".into()),
        Err(RuntimeError::Killed(kill)) => (Level::WARN, format!("was killed: {kill}")),
        Err(RuntimeError::Stopped) => (Level::WARN, "ended because its player stopped".into()),
        Err(error) => (Level::WARN, format!("ended: {error}")),
    };
    (level, format!("{name} {end}"))
}
fn with_reason(end: &str, reason: Option<String>) -> String {
    match reason {
        Some(reason) => format!("{end}: {reason}"),
        None => end.into(),
    }
}

fn random_private_room() -> u32 {
    use std::sync::atomic::AtomicU64;
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let draw = SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ^ std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
    10000 + (draw % 90000) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outcome_lines_name_the_script_and_the_end() {
        for (result, level, line) in [
            (Ok(ScriptOutcome::Completed), Level::INFO, "x.js completed"),
            (
                Ok(ScriptOutcome::Failed {
                    message: "boom".into(),
                    details: None,
                }),
                Level::WARN,
                "x.js failed: boom",
            ),
            (
                Ok(ScriptOutcome::Failed {
                    message: "boom".into(),
                    details: Some("stack".into()),
                }),
                Level::WARN,
                "x.js failed: boom\nstack",
            ),
            (
                Ok(ScriptOutcome::StoppedByScript { reason: None }),
                Level::INFO,
                "x.js stopped itself",
            ),
            (
                Ok(ScriptOutcome::StoppedByScript {
                    reason: Some("done".into()),
                }),
                Level::INFO,
                "x.js stopped itself: done",
            ),
            (
                Ok(ScriptOutcome::Exited {
                    logout: false,
                    close_client: false,
                }),
                Level::INFO,
                "x.js exited",
            ),
            (
                Ok(ScriptOutcome::Exited {
                    logout: false,
                    close_client: true,
                }),
                Level::INFO,
                "x.js exited and asked to close the game client",
            ),
            (
                Ok(ScriptOutcome::Stopped { reason: None }),
                Level::INFO,
                "x.js was stopped",
            ),
            (
                Ok(ScriptOutcome::Stopped {
                    reason: Some("enough".into()),
                }),
                Level::INFO,
                "x.js was stopped: enough",
            ),
            (
                Err(RuntimeError::Killed(Kill::Requested)),
                Level::WARN,
                "x.js was killed",
            ),
            (
                Err(RuntimeError::Killed(Kill::Watchdog)),
                Level::WARN,
                "x.js was killed: a turn used more than 200 ms of CPU time",
            ),
            (
                Err(RuntimeError::Killed(Kill::HeapLimit)),
                Level::WARN,
                "x.js was killed: it reached the 64 MiB heap limit",
            ),
            (
                Err(RuntimeError::Stopped),
                Level::WARN,
                "x.js ended because its player stopped",
            ),
            (
                Err(RuntimeError::EventsLost),
                Level::WARN,
                "x.js ended: script events overflowed and were lost",
            ),
            (
                Err(RuntimeError::JavaScript("boom".into())),
                Level::WARN,
                "x.js ended: boom",
            ),
        ] {
            assert_eq!(ended("x.js", result), (level, line.into()));
        }
    }
}
