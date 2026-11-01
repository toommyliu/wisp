use std::sync::Arc;

use lucent_protocol::script::{Script, ScriptInputs};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionEpoch(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub player: PlayerId,
    pub connection: ConnectionEpoch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparationId(pub u64);

#[derive(Clone, Debug)]
pub struct Request {
    pub name: String,
    pub reference: String,
    pub script: Arc<Script>,
    pub inputs: ScriptInputs,
}
impl PartialEq for Request {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.reference == other.reference
            && self.inputs == other.inputs
            && Arc::ptr_eq(&self.script, &other.script)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: EntryId,
    pub request: Request,
    pub inputs_available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomPolicy {
    Public,
    RandomPrivate,
    Specific(u32),
}
#[derive(Clone, Debug)]
pub struct Options {
    pub room_policy: RoomPolicy,
    pub safe_start_stop: bool,
    pub restart_after_reconnect: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Readiness {
    pub logged_in: bool,
    pub username: String,
    pub projections_ready: bool,
}
impl Readiness {
    pub(super) fn ready(&self) -> bool {
        self.logged_in && !self.username.trim().is_empty() && self.projections_ready
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    Requested(String),
    Script(Option<String>),
    ConnectionLost,
    AccountChanged,
    PlayerReplaced,
    GraceExpired,
}
impl StopReason {
    pub fn message(&self) -> &str {
        match self {
            Self::Requested(reason) => reason,
            Self::Script(reason) => reason.as_deref().unwrap_or("Stopped by script"),
            Self::ConnectionLost => "Connection lost",
            Self::AccountChanged => "Account changed",
            Self::PlayerReplaced => "Player replaced",
            Self::GraceExpired => "did not stop within 5 s",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillReason {
    Requested,
    Watchdog,
    Heap,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Failed(String),
    Stopped(StopReason),
    Exited { logout: bool, close_client: bool },
    Killed(KillReason),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terminal {
    pub run: RunId,
    pub name: String,
    pub outcome: Outcome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafePhase {
    Before,
    After,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunPhase {
    Waiting,
    SafeBefore,
    Booting,
    Running,
    Stopping,
    Finishing,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveStatus {
    pub run: RunId,
    pub name: String,
    pub phase: RunPhase,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub active: Option<ActiveStatus>,
    pub waiting_to_restart: bool,
    pub latest: Option<Terminal>,
    pub queue: super::queue::QueueStatus,
}
#[derive(Clone, Debug)]
pub enum Command {
    Start(Request),
    Queue(super::queue::Command),
    Stop { run: RunId, reason: String },
    Kill { run: RunId },
}
#[derive(Clone, Debug)]
pub enum Event {
    Command(Command),
    Prepared {
        id: PreparationId,
        result: Result<Entry, String>,
    },
    ProjectionChanged,
    PlayerReady {
        run: RunId,
        ready: bool,
    },
    RunReady(RunId),
    RunExited {
        run: RunId,
        outcome: Outcome,
    },
    Reaped(RunId),
    ConnectionLost(Binding),
    ConnectionRestored(Binding),
    AccountChanged,
    PlayerReplaced(Binding),
    SafeMoveDone {
        run: RunId,
        phase: SafePhase,
    },
    LogoutDone(RunId),
    OptionsChanged,
    Deadline,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Prepare {
        id: PreparationId,
        entry: Entry,
    },
    ReadPlayerReady {
        run: RunId,
        binding: Binding,
        deadline: u64,
    },
    Spawn {
        run: RunId,
        binding: Binding,
        request: Request,
    },
    SignalStop {
        run: RunId,
    },
    Kill {
        run: RunId,
    },
    Revoke {
        run: RunId,
    },
    Reap {
        run: RunId,
    },
    CancelSafeMove {
        run: RunId,
        phase: SafePhase,
    },
    SafeMove {
        run: RunId,
        binding: Binding,
        phase: SafePhase,
        room_policy: RoomPolicy,
    },
    Logout {
        run: RunId,
        binding: Binding,
    },
    Terminal(Terminal),
    Publish(Status),
    ScheduleDeadline(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Busy,
    Empty,
    Full,
    DuplicateEntry,
    IdExhausted,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transition {
    pub effects: Vec<Effect>,
    pub rejection: Option<Rejection>,
}
