use crate::game::travel;
use std::num::NonZeroU32;

pub use lucent_protocol::follower::{
    FollowerAttackTarget as AttackTarget, FollowerPhase as Phase, FollowerStatus as Status,
};

#[derive(Clone, Debug)]
pub struct Config {
    pub target_name: String,
    pub combat_enabled: bool,
    pub copy_walk: bool,
    pub retry_enabled: bool,
    pub max_attempts: NonZeroU32,
    pub attack_priority: Vec<AttackTarget>,
    pub locked_zone_fallbacks: Vec<String>,
    pub locked_zone_room_override: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            target_name: String::new(),
            combat_enabled: true,
            copy_walk: false,
            retry_enabled: true,
            max_attempts: NonZeroU32::new(3).unwrap(),
            attack_priority: vec![],
            locked_zone_fallbacks: vec![],
            locked_zone_room_override: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub map: String,
    pub cell: String,
    pub pad: String,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug)]
pub struct Player {
    pub username: String,
    pub location: Location,
    pub position: Option<Position>,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub can_act: bool,
    pub me: Option<Player>,
    pub target: Option<Player>,
    pub random_room: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId(pub u64);
#[derive(Clone, Debug)]
pub enum Action {
    Travel(travel::Request),
    GoTo(String),
    Walk(Position),
}
#[derive(Clone, Debug)]
pub enum Effect {
    Issue {
        id: RequestId,
        action: Action,
    },
    Cancel {
        id: RequestId,
    },
    StartCombat {
        id: RequestId,
        priorities: Vec<AttackTarget>,
        fallback_rank: usize,
    },
    EngageCombat {
        id: RequestId,
        engaged: bool,
    },
    StopCombat {
        id: Option<RequestId>,
    },
}
#[derive(Clone, Debug)]
pub enum Reply {
    Returned(bool),
    Error(String),
}
#[derive(Clone, Debug)]
pub enum LocationKind {
    Cell,
    Walk(Option<Position>),
    Position(Position),
}
#[derive(Clone, Debug)]
pub enum CombatEvent {
    Attacking,
    Attacked,
    DispatchError,
    AsyncFailure(String),
    Failed(String),
    Warnings(Vec<String>),
}
#[derive(Clone, Debug)]
pub enum Event {
    Start,
    Stop,
    Reset,
    Tick,
    Completed {
        id: RequestId,
        reply: Reply,
    },
    TargetLocation {
        username: String,
        location: Location,
        kind: LocationKind,
    },
    OwnMapJoined,
    ConnectionChanged,
    PlayersChanged,
    ReadinessChanged,
    ServerMessage {
        command: String,
        message: String,
    },
    Combat {
        id: RequestId,
        event: CombatEvent,
    },
}
#[derive(Default, Debug)]
pub struct Effects {
    pub actions: Vec<Effect>,
    pub next_deadline: Option<u64>,
}
