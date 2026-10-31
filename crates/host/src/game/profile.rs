mod conditions;
mod effects;
mod events;
mod execution;
mod preparation;
mod rotation;

pub use conditions::*;
pub use effects::*;
pub use preparation::*;
pub use rotation::*;

#[cfg(test)]
mod regression_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod timing_tests;

use lucent_protocol::combat_profile::CombatProfile;

pub const RETRY_MS: u64 = 250;
pub const TARGET_ABSENCE_GRACE_MS: u64 = 500;
pub const MAX_STEPS: usize = 256;
pub const MAX_TRIGGERS: usize = 128;
pub const MAX_CONDITIONS: usize = 32;
pub const MAX_MONSTERS: usize = 2048;
pub const MAX_ALLIES: usize = 1024;
pub const MAX_AURAS: usize = 128;

#[derive(Clone, Copy, Debug)]
enum TargetState {
    Unselected,
    Selected {
        lease: TargetLease,
        confirmation: Confirmation,
    },
}

#[derive(Clone, Copy, Debug)]
enum Confirmation {
    Unconfirmed,
    Confirmed { missing_since: Option<u64> },
}

#[derive(Clone, Debug)]
enum Pending {
    Prepare,
    Attack(TargetLease),
    Cast {
        lease: TargetLease,
        cast: PreparedCast,
    },
    Trigger {
        target: Option<TargetLease>,
        index: usize,
        started: u64,
        message: Message,
    },
}

impl Pending {
    fn target(&self) -> Option<TargetLease> {
        match self {
            Self::Attack(lease) | Self::Cast { lease, .. } => Some(*lease),
            Self::Trigger { target, .. } => *target,
            Self::Prepare => None,
        }
    }
    fn stage(&self) -> FailureStage {
        match self {
            Self::Prepare => FailureStage::Preparation,
            Self::Attack(_) => FailureStage::Attack,
            Self::Cast { .. } => FailureStage::ProfileCast,
            Self::Trigger { .. } => FailureStage::MessageTrigger,
        }
    }
}

#[derive(Clone, Debug)]
enum Phase {
    New,
    Ready,
    Pending {
        id: ActionId,
        action: Pending,
    },
    Casting(TargetLease),
    Cooldown {
        lease: TargetLease,
        cast: PreparedCast,
        expires_at: u64,
    },
    TriggerCooldown {
        target: Option<TargetLease>,
        index: usize,
        started: u64,
        message: Message,
        expires_at: u64,
    },
    Triggering {
        target: Option<TargetLease>,
        index: usize,
        message: Message,
    },
    Waiting {
        until: u64,
    },
    Reconfirm {
        pending: Pending,
        expires_at: u64,
        until: u64,
    },
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct ProfileJob {
    profile: CombatProfile,
    rotation: Rotation,
    last_triggers: Vec<Option<u64>>,
    target: TargetState,
    version: u64,
    sequence: u64,
    phase: Phase,
    next_step: u64,
    fallback_rank: Option<usize>,
    skill5_item_id: Option<i64>,
    warnings: Vec<String>,
    engaged: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileLimit {
    Steps,
    Conditions,
    Triggers,
}

impl ProfileJob {
    pub fn new(profile: CombatProfile, fallback_rank: Option<usize>) -> Result<Self, ProfileLimit> {
        if profile.steps.len() > MAX_STEPS {
            return Err(ProfileLimit::Steps);
        }
        if profile
            .steps
            .iter()
            .any(|step| step.conditions.len() > MAX_CONDITIONS)
        {
            return Err(ProfileLimit::Conditions);
        }
        let triggers = profile.message_triggers.as_ref().map_or(0, Vec::len);
        if triggers > MAX_TRIGGERS {
            return Err(ProfileLimit::Triggers);
        }
        Ok(Self {
            profile,
            rotation: Rotation::default(),
            last_triggers: vec![None; triggers],
            target: TargetState::Unselected,
            version: 0,
            sequence: 0,
            phase: Phase::New,
            next_step: 0,
            fallback_rank,
            skill5_item_id: None,
            warnings: Vec::new(),
            engaged: true,
        })
    }

    pub fn profile(&self) -> &CombatProfile {
        &self.profile
    }

    pub fn set_engaged(&mut self, engaged: bool) {
        self.engaged = engaged;
        if !engaged
            && matches!(
                self.phase,
                Phase::Casting(_)
                    | Phase::Cooldown { .. }
                    | Phase::Waiting { .. }
                    | Phase::Reconfirm {
                        pending: Pending::Cast { .. },
                        ..
                    }
            )
        {
            self.phase = Phase::Ready;
        }
    }

    pub(crate) fn invalidate(&mut self, prepare: bool, now: u64) {
        self.lose_target(false);
        self.phase = if prepare
            || matches!(
                self.phase,
                Phase::New
                    | Phase::Pending {
                        action: Pending::Prepare,
                        ..
                    }
            ) {
            self.skill5_item_id = None;
            Phase::New
        } else {
            Phase::Ready
        };
        self.next_step = now;
    }

    pub fn ready(&self) -> bool {
        matches!(self.phase, Phase::Ready)
    }

    pub fn needs_observation(&self) -> bool {
        match self.phase {
            Phase::New | Phase::Pending { .. } | Phase::Waiting { .. } | Phase::Cancelled => false,
            Phase::Ready => !self.profile.steps.is_empty(),
            _ => true,
        }
    }

    pub fn target_version(&self) -> u64 {
        self.version
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    fn issue(&mut self, action: Action, pending: Pending) -> ProfileEffects {
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("profile action identity exhausted");
        let id = ActionId(self.sequence);
        self.phase = Phase::Pending {
            id,
            action: pending,
        };
        ProfileEffects {
            command: Some(Command { id, action }),
            ..Default::default()
        }
    }

    fn cycle(&mut self, cycle: Cycle, now: u64, extra_wait: u32) -> ProfileEffects {
        let delay = match cycle {
            Cycle::Attacked { cast: true } => u64::from(self.profile.delay_ms.max(50)),
            _ => RETRY_MS,
        };
        if extra_wait > 0 {
            let until = now.saturating_add(u64::from(extra_wait));
            self.phase = Phase::Waiting { until };
            return ProfileEffects {
                next_deadline: Some(until),
                ..Default::default()
            };
        }
        self.next_step = now.saturating_add(delay);
        self.phase = Phase::Ready;
        ProfileEffects {
            outcome: Some(Outcome::Cycle(cycle)),
            next_deadline: Some(self.next_step),
            ..Default::default()
        }
    }

    fn lose_target(&mut self, reset: bool) {
        self.version = self
            .version
            .checked_add(1)
            .expect("profile target generation exhausted");
        self.target = TargetState::Unselected;
        if reset {
            self.rotation.reset();
        }
    }

    fn lease_current(&self, snapshot: &ProfileSnapshot, lease: TargetLease) -> bool {
        lease.version == self.version
            && snapshot.monsters.iter().all(|monster| {
                monster.key.monster_map_id != lease.key.monster_map_id
                    || (monster.key == lease.key && monster.living())
            })
    }

    pub fn advance(
        &mut self,
        snapshot: &ProfileSnapshot,
        event: ProfileEvent,
        now: u64,
    ) -> ProfileEffects {
        if matches!(self.phase, Phase::Cancelled) {
            return ProfileEffects::default();
        }
        if matches!(event, ProfileEvent::Cancel) {
            let release = !matches!(self.phase, Phase::New)
                && !(self.profile.steps.is_empty() && self.last_triggers.is_empty());
            self.lose_target(false);
            self.phase = Phase::Cancelled;
            self.sequence += 1;
            return ProfileEffects {
                command: release.then_some(Command {
                    id: ActionId(self.sequence),
                    action: Action::ReleasePreparation,
                }),
                outcome: Some(Outcome::Cancelled),
                next_deadline: None,
            };
        }
        if let ProfileEvent::MonsterDeath(key) = event {
            if let TargetState::Selected {
                lease,
                confirmation,
            } = self.target
                && lease.key == key
            {
                self.lose_target(
                    matches!(confirmation, Confirmation::Confirmed { .. })
                        && self.profile.reset_skill_index_on_target_death == Some(true),
                );
                return ProfileEffects {
                    outcome: Some(Outcome::TargetLost(lease)),
                    next_deadline: match self.phase {
                        Phase::Pending { .. } => None,
                        Phase::Waiting { until } => Some(until),
                        _ => Some(self.next_step.max(now)),
                    },
                    ..Default::default()
                };
            }
            return ProfileEffects::default();
        }
        if matches!(event, ProfileEvent::ResetRotation) {
            self.rotation.reset();
            return ProfileEffects::default();
        }
        let oversized = snapshot.monsters.len() > MAX_MONSTERS
            || snapshot.allies.len() > MAX_ALLIES
            || snapshot.self_auras.len() > MAX_AURAS
            || snapshot
                .monsters
                .iter()
                .any(|monster| monster.auras.len() > MAX_AURAS);
        if !oversized
            && let Phase::Pending { action, .. } = &self.phase
            && let Some(lease) = action.target()
            && lease.version == self.version
            && !self.lease_current(snapshot, lease)
        {
            self.lose_target(false);
        }
        let event = match event {
            ProfileEvent::Completed { id, reply } => {
                if let Some(effects) = self.complete_action(id, reply, now) {
                    return effects;
                }
                ProfileEvent::Tick
            }
            event => event,
        };
        let observation_failure = match event {
            ProfileEvent::ObservationFailed(ref message) => Some(message.clone()),
            _ if oversized => Some("Combat profile snapshot exceeds limits".into()),
            _ => None,
        };
        if let Some(message) = observation_failure {
            self.next_step = self.next_step.max(now.saturating_add(RETRY_MS));
            return ProfileEffects {
                outcome: Some(Outcome::Failure {
                    stage: FailureStage::TargetSelection,
                    message,
                }),
                next_deadline: Some(match self.phase {
                    Phase::Waiting { until } => until.min(self.next_step),
                    _ => self.next_step,
                }),
                ..Default::default()
            };
        }
        match event {
            ProfileEvent::Message {
                observed_version,
                message,
            } => {
                if let Some(effects) = self.start_message(snapshot, observed_version, message) {
                    return effects;
                }
            }
            ProfileEvent::Tick => {}
            ProfileEvent::MonsterDeath(_)
            | ProfileEvent::Completed { .. }
            | ProfileEvent::ResetRotation
            | ProfileEvent::Cancel
            | ProfileEvent::ObservationFailed(_) => unreachable!(),
        }
        self.advance_phase(snapshot, now)
    }
}
