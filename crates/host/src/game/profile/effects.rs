use super::{CastReadiness, Message, TargetKey};
use lucent_protocol::combat_profile::SkillSlot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionId(pub(super) u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetLease {
    pub key: TargetKey,
    pub version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Prepare,
    Attack(TargetLease),
    UseSkill {
        skill: SkillSlot,
        target: Option<TargetLease>,
        readiness: CastReadiness,
        force: CastForce,
    },
    ReleasePreparation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastForce {
    Normal,
    Forced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: ActionId,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Prepared {
        skill5_item_id: Option<i64>,
        warnings: Vec<String>,
    },
    Accepted(bool),
    Abandoned {
        target_lost: bool,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileEvent {
    Tick,
    Completed {
        id: ActionId,
        reply: Reply,
    },
    MonsterDeath(TargetKey),
    Message {
        observed_version: u64,
        message: Message,
    },
    ResetRotation,
    Cancel,
    ObservationFailed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureStage {
    Preparation,
    TargetSelection,
    Attack,
    ProfileCast,
    MessageTrigger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cycle {
    Idle,
    PlayerDead,
    NoTarget,
    AttackRejected,
    Attacked { cast: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Cycle(Cycle),
    Failure {
        stage: FailureStage,
        message: String,
    },
    TargetLost(TargetLease),
    Cancelled,
    Deferred,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileEffects {
    pub command: Option<Command>,
    pub outcome: Option<Outcome>,
    pub next_deadline: Option<u64>,
}
