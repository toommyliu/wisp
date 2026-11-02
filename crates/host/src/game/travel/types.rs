use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Request {
    Join {
        target: String,
        cell: Option<String>,
        pad: Option<String>,
        random_room: Option<u32>,
    },
    Jump {
        cell: String,
        pad: Option<String>,
    },
    Exit,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CombatState {
    #[default]
    Idle,
    Combat,
    Dead,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Monster {
    pub cell: String,
    pub aggressive: bool,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Snapshot {
    pub map: String,
    pub room: u64,
    pub loaded: bool,
    pub ready: bool,
    pub tfer: bool,
    pub alive: bool,
    pub state: CombatState,
    pub cell: String,
    pub pad: String,
    pub live_cell: String,
    pub live_pad: String,
    pub cells: Vec<String>,
    pub monsters: Vec<Monster>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Join(Vec<String>),
    Jump(Vec<String>),
    CellReady(Vec<String>),
    CancelAutoAttack,
    CancelTarget,
}
impl Action {
    pub fn loader_call(&self) -> (&str, Vec<String>) {
        match self {
            Self::Join(args) => ("player.joinMap", args.clone()),
            Self::Jump(args) => ("player.jump", args.clone()),
            Self::CellReady(args) => ("world.isCellReady", args.clone()),
            Self::CancelAutoAttack => ("combat.cancelAutoAttack", vec![]),
            Self::CancelTarget => ("combat.cancelTarget", vec![]),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Command {
    pub id: u64,
    pub action: Action,
}
#[derive(Clone, Copy, Debug)]
pub enum Event {
    Tick,
    Completed { id: u64, reply: Reply },
    Cancel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Returned(bool),
    Cancelled,
}
#[derive(Default, Debug)]
pub struct Effects {
    pub command: Option<Command>,
    pub outcome: Option<Outcome>,
    pub next_deadline: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    TooLarge,
    RandomRoomRequired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Accepted,
    Unavailable,
    CellReady(bool),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reads(u8);
impl Reads {
    pub const LOAD: Self = Self(1);
    pub const READY: Self = Self(2);
    pub const TFER: Self = Self(4);
    pub const ALIVE: Self = Self(8);
    pub const STATE: Self = Self(16);
    pub const LIVE: Self = Self(32);
    pub const EXIT: Self = Self(64);
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}
