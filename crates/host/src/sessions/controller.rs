use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    task::{Wake, Waker},
    thread,
    time::{Duration, Instant},
};

use crate::{
    HostError, ScriptLink, Session, Surface,
    automation::{AutoZonePolicy, ReloginPolicy, SettingsPolicy},
    bridge, reap,
};
use lucent_protocol::script::{Script, ScriptOutcome};
use tracing::{Level, info, warn};

use crate::scripting::{Kill, RuntimeError, ScriptRun};

pub struct ScriptRequest {
    script: Script,
    inbox: mpsc::Sender<Msg>,
    messages: mpsc::Receiver<Msg>,
}

#[derive(Clone)]
pub struct ScriptControls(mpsc::Sender<Msg>);

impl ScriptRequest {
    pub fn new(script: Script) -> (Self, ScriptControls) {
        let (inbox, messages) = mpsc::channel();
        let controls = ScriptControls(inbox.clone());
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
        let _ = self.0.send(Msg::Stop);
    }
    pub fn kill(&self) {
        let _ = self.0.send(Msg::Kill);
    }
    pub fn status(&self) {
        let _ = self.0.send(Msg::Status);
    }
}

pub struct Supervisor {
    inbox: mpsc::Sender<Msg>,
    has_script: bool,
    thread: Option<thread::JoinHandle<()>>,
}

enum Msg {
    Player(Box<Player>),
    Wake,
    Start(Box<(Script, ScriptLink)>),
    RunReady(u64),
    RunExited(u64),
    Reaped(u64),
    Stop,
    Kill,
    Status,
    Shutdown,
}
struct Player {
    watch: Option<ScriptLink>,
    relogin: ScriptLink,
    auto_zone: ScriptLink,
    jobs: ScriptLink,
}

impl Supervisor {
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
        let has_script = script.is_some();
        let first = attach(session, has_script).ok_or(RuntimeError::Stopped)?;
        let supervision = Supervision {
            session: first.relogin.id(),
            notify: Arc::new(Notify {
                inbox: inbox.clone(),
                queued: AtomicBool::new(false),
            }),
            script: match script {
                Some(script) => ScriptState::Waiting {
                    script,
                    watch: None,
                },
                None => ScriptState::Idle,
            },
            next_run: 0,
            settings: None,
            relogin: None,
            auto_zone: None,
            jobs: None,
            deadline: None,
        };
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let thread = thread::Builder::new()
            .name(format!("lucent-supervisor-{}", first.relogin.id()))
            .spawn(move || {
                let _dispatch = tracing::dispatcher::set_default(&dispatch);
                supervision.run(first, messages);
            })?;
        Ok(Self {
            inbox,
            has_script,
            thread: Some(thread),
        })
    }

    pub fn start_script(
        &mut self,
        session: &mut Session,
        script: Script,
    ) -> Result<ScriptControls, RuntimeError> {
        let watch = session
            .attach(bridge::EventInterests::ALL)
            .ok_or(RuntimeError::Stopped)?;
        self.has_script = true;
        self.inbox
            .send(Msg::Start(Box::new((script, watch))))
            .map_err(|_| RuntimeError::Stopped)?;
        Ok(ScriptControls(self.inbox.clone()))
    }

    pub fn restart_player(
        &mut self,
        session: &mut Session,
        surface: Surface,
    ) -> Result<(), HostError> {
        session.restart(surface)?;
        if let Some(player) = attach(session, self.has_script) {
            let _ = self.inbox.send(Msg::Player(Box::new(player)));
        }
        Ok(())
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.inbox.send(Msg::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn attach(session: &mut Session, has_script: bool) -> Option<Player> {
    let watch = if has_script {
        Some(session.attach(bridge::EventInterests::ALL)?)
    } else {
        None
    };
    Some(Player {
        watch,
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

struct Supervision {
    session: u64,
    notify: Arc<Notify>,
    script: ScriptState,
    next_run: u64,
    settings: Option<SettingsPolicy>,
    relogin: Option<ReloginPolicy>,
    auto_zone: Option<AutoZonePolicy>,
    jobs: Option<crate::game::jobs::ProfileJobs>,
    deadline: Option<Instant>,
}
struct CurrentRun {
    id: u64,
    name: String,
    run: ScriptRun,
}
enum ScriptState {
    Idle,
    Waiting {
        script: Script,
        watch: Option<ScriptLink>,
    },
    Booting(CurrentRun),
    Running(CurrentRun),
    Reaping {
        id: u64,
        next: Option<(Script, ScriptLink)>,
    },
}

struct RunNotify {
    inbox: mpsc::Sender<Msg>,
    id: u64,
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
    fn run(mut self, first: Player, messages: mpsc::Receiver<Msg>) {
        self.player(first);
        loop {
            self.settle();
            let message = match self.deadline {
                Some(deadline) => {
                    messages.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                }
                None => messages
                    .recv()
                    .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
            };
            match message {
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) | Ok(Msg::Shutdown) => break,
                Ok(Msg::Player(player)) => self.player(*player),
                Ok(Msg::Wake) => self.notify.queued.store(false, Ordering::Release),
                Ok(Msg::Start(request)) => {
                    let (script, watch) = *request;
                    self.request(script, watch);
                }
                Ok(Msg::RunReady(id)) => {
                    if matches!(&self.script, ScriptState::Booting(run) if run.id == id) {
                        let ScriptState::Booting(run) =
                            std::mem::replace(&mut self.script, ScriptState::Idle)
                        else {
                            unreachable!()
                        };
                        if run.run.take_ready() {
                            info!("session {}: {} started", self.session, run.name);
                            self.script = ScriptState::Running(run);
                        } else {
                            self.script = ScriptState::Booting(run);
                        }
                    }
                }
                Ok(Msg::RunExited(id)) => {
                    if matches!(&self.script, ScriptState::Booting(run) | ScriptState::Running(run) if run.id == id)
                    {
                        self.finish_run(None);
                    }
                }
                Ok(Msg::Reaped(id)) => {
                    if matches!(&self.script, ScriptState::Reaping { id: current, .. } if *current == id)
                    {
                        let ScriptState::Reaping { next, .. } =
                            std::mem::replace(&mut self.script, ScriptState::Idle)
                        else {
                            unreachable!()
                        };
                        if let Some((script, watch)) = next {
                            self.script = ScriptState::Waiting {
                                script,
                                watch: Some(watch),
                            };
                        }
                        tracing::debug!("session {}: script worker reaped", self.session);
                    }
                }
                Ok(Msg::Stop) => self.control(false),
                Ok(Msg::Kill) => self.control(true),
                Ok(Msg::Status) => self.status(),
            }
        }
        if matches!(
            self.script,
            ScriptState::Booting(_) | ScriptState::Running(_)
        ) {
            self.finish_run(Some(Err(RuntimeError::Stopped)));
        }
    }

    fn player(&mut self, player: Player) {
        let Player {
            watch,
            relogin,
            auto_zone,
            jobs,
        } = player;
        if matches!(
            self.script,
            ScriptState::Booting(_) | ScriptState::Running(_)
        ) {
            self.finish_run(Some(Err(RuntimeError::Stopped)));
        }
        if let ScriptState::Reaping { next, .. } = &mut self.script {
            *next = None;
        }
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

        if let ScriptState::Waiting {
            script,
            watch: slot,
        } = &mut self.script
        {
            if let Some(link) = &watch {
                link.set_waker(Waker::from(self.notify.clone()));
            }
            *slot = watch;
            info!(
                "session {}: {} is waiting for login",
                self.session,
                script.entry()
            );
        }
    }

    fn request(&mut self, script: Script, watch: ScriptLink) {
        watch.set_waker(Waker::from(self.notify.clone()));
        match &mut self.script {
            ScriptState::Idle => {
                self.script = ScriptState::Waiting {
                    script,
                    watch: Some(watch),
                }
            }
            ScriptState::Reaping {
                next: slot @ None, ..
            } => *slot = Some((script, watch)),
            _ => warn!("session {}: a script is already active", self.session),
        }
    }

    fn settle(&mut self) {
        self.deadline = self
            .settings
            .as_mut()
            .and_then(|settings| settings.advance(Instant::now()));
        if let Some(at) = self
            .relogin
            .as_mut()
            .and_then(|policy| policy.advance(Instant::now()))
        {
            self.deadline = Some(self.deadline.map_or(at, |deadline| deadline.min(at)));
        }
        self.deadline = self
            .deadline
            .into_iter()
            .chain(
                self.auto_zone
                    .as_mut()
                    .and_then(|policy| policy.advance(Instant::now())),
            )
            .min();
        if let ScriptState::Waiting {
            watch: Some(watch), ..
        } = &self.script
        {
            match logged_in(watch) {
                Ok(WatchReadiness::Ready) => self.start_run(),
                Ok(WatchReadiness::Waiting) => {}
                Ok(WatchReadiness::Draining) => self.deadline = Some(Instant::now()),
                Err(bridge::Error::Timeout) => {
                    self.deadline = Some(Instant::now() + Duration::from_millis(1))
                }
                Err(_) => {
                    if let ScriptState::Waiting { watch, .. } = &mut self.script {
                        *watch = None;
                    }
                }
            }
        }
        self.deadline = self
            .deadline
            .into_iter()
            .chain(
                self.jobs
                    .as_mut()
                    .and_then(|jobs| jobs.advance(Instant::now())),
            )
            .min();
    }

    fn start_run(&mut self) {
        let ScriptState::Waiting {
            script,
            watch: Some(watch),
        } = std::mem::replace(&mut self.script, ScriptState::Idle)
        else {
            unreachable!()
        };
        let name = script.entry().to_string();
        self.next_run += 1;
        let id = self.next_run;
        let notify = |ready| {
            Waker::from(Arc::new(RunNotify {
                inbox: self.notify.inbox.clone(),
                id,
                ready,
            }))
        };
        match ScriptRun::spawn(
            watch,
            script,
            Default::default(),
            notify(true),
            notify(false),
        ) {
            Ok(run) => {
                self.script = ScriptState::Booting(CurrentRun { id, name, run });
            }
            Err(error) => {
                self.ended(&name, Err(error));
            }
        }
    }

    fn finish_run(&mut self, result: Option<Result<ScriptOutcome, RuntimeError>>) {
        let run = match std::mem::replace(&mut self.script, ScriptState::Idle) {
            ScriptState::Booting(run) | ScriptState::Running(run) => run,
            _ => unreachable!(),
        };
        let result = result.unwrap_or_else(|| run.run.wait(Duration::ZERO));
        self.ended(&run.name, result);
        let inbox = self.notify.inbox.clone();
        let id = run.id;
        self.script = ScriptState::Reaping { id, next: None };
        reap(move || {
            drop(run);
            let _ = inbox.send(Msg::Reaped(id));
        });
    }

    fn control(&mut self, kill: bool) {
        match &mut self.script {
            ScriptState::Booting(CurrentRun { name, run, .. })
            | ScriptState::Running(CurrentRun { name, run, .. }) => {
                if kill {
                    run.kill();
                    info!("session {}: killing {name}", self.session);
                } else {
                    run.stop_script();
                    info!("session {}: stopping {name}", self.session);
                }
            }
            ScriptState::Waiting { script, .. } => {
                info!(
                    "session {}: {} was cancelled before it started",
                    self.session,
                    script.entry()
                );
                self.script = ScriptState::Idle;
            }
            ScriptState::Reaping { next, .. } => {
                *next = None;
            }
            ScriptState::Idle => info!("session {}: no script is running", self.session),
        }
    }

    fn status(&mut self) {
        match &self.script {
            ScriptState::Idle | ScriptState::Reaping { .. } => {
                info!("session {}: no script is running", self.session)
            }
            ScriptState::Waiting { script, .. } => info!(
                "session {}: {} is waiting for login",
                self.session,
                script.entry()
            ),
            ScriptState::Booting(run) => info!("session {}: {} is booting", self.session, run.name),
            ScriptState::Running(run) => info!("session {}: {} is running", self.session, run.name),
        }
    }

    fn ended(&self, name: &str, result: Result<ScriptOutcome, RuntimeError>) {
        let (level, line) = ended(name, result);
        if level == Level::WARN {
            warn!("session {}: {line}", self.session);
        } else {
            info!("session {}: {line}", self.session);
        }
    }
}

enum WatchReadiness {
    Ready,
    Waiting,
    Draining,
}

fn logged_in(link: &ScriptLink) -> Result<WatchReadiness, bridge::Error> {
    for _ in 0..64 {
        match link.try_event() {
            Ok(Some(_)) | Err(bridge::Error::Overflow) => {}
            Ok(None) => {
                return link.read_for(Duration::ZERO, |projection| {
                    if projection.current_readiness().missing.is_empty() {
                        WatchReadiness::Ready
                    } else {
                        WatchReadiness::Waiting
                    }
                });
            }
            Err(error) => return Err(error),
        }
    }
    Ok(WatchReadiness::Draining)
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
        }) => (Level::INFO, "exited".into()),
        Ok(ScriptOutcome::Exited { close_client: true }) => (
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
                    close_client: false,
                }),
                Level::INFO,
                "x.js exited",
            ),
            (
                Ok(ScriptOutcome::Exited { close_client: true }),
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
