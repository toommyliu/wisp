use super::ScriptEvent;
use lucent_protocol::event::ProjectionEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Job,
    ProfileDeath,
    Loader,
    Connection,
    Invalidated,
    AutoZoneChanged,
    OperationResponsibility,
    OperationStarted,
    OperationIssued,
    Operation,
    Login,
    Logout,
    PlayerAfk,
    JoinMap,
    ItemDrop,
    QuestComplete,
    MonsterDeath,
    MonsterRespawn,
    PlayerDeath,
    PlayersChanged,
    AuraAdded,
    AuraUpdated,
    AuraRemoved,
    CounterAttackStart,
    CounterAttackEnd,
    PlayerLocation,
    UpdateMessage,
    Zone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventInterests(u64);

impl EventInterests {
    pub const ALL: Self = Self(u64::MAX);

    pub const fn new(kinds: &[EventKind]) -> Self {
        let mut bits = 0;
        let mut i = 0;
        while i < kinds.len() {
            bits |= 1 << kinds[i] as u8;
            i += 1;
        }
        Self(bits)
    }

    fn contains(self, kind: EventKind) -> bool {
        self.0 & (1 << kind as u8) != 0
    }

    pub(crate) fn accepts(self, event: &ScriptEvent) -> bool {
        if matches!(event, ScriptEvent::ProfileDeath { .. }) {
            return self != Self::ALL && self.contains(EventKind::ProfileDeath);
        }
        if self == Self::ALL {
            return true;
        }
        let kind = match event {
            ScriptEvent::ProfileDeath { .. } => EventKind::ProfileDeath,
            ScriptEvent::TravelDone { .. }
            | ScriptEvent::JobPrepared { .. }
            | ScriptEvent::JobWarning { .. }
            | ScriptEvent::JobFailed { .. } => EventKind::Job,
            ScriptEvent::OperationResponsibility { .. } => EventKind::OperationResponsibility,
            ScriptEvent::OperationStarted { .. } => EventKind::OperationStarted,
            ScriptEvent::OperationIssued { .. } => EventKind::OperationIssued,
            ScriptEvent::Operation { .. } => EventKind::Operation,
            ScriptEvent::AutoZoneChanged { .. } => EventKind::AutoZoneChanged,
            ScriptEvent::Invalidated => EventKind::Invalidated,
            ScriptEvent::Loader { event, projected } => {
                return self.contains(EventKind::Loader)
                    || (event.name == "onConnection" && self.contains(EventKind::Connection))
                    || projected.iter().any(|event| self.contains(event.into()));
            }
        };
        self.contains(kind)
    }
}

impl From<&ProjectionEvent> for EventKind {
    fn from(event: &ProjectionEvent) -> Self {
        match event {
            ProjectionEvent::Login => Self::Login,
            ProjectionEvent::Logout => Self::Logout,
            ProjectionEvent::PlayerAfk { .. } => Self::PlayerAfk,
            ProjectionEvent::JoinMap { .. } => Self::JoinMap,
            ProjectionEvent::ItemDrop { .. } => Self::ItemDrop,
            ProjectionEvent::QuestComplete { .. } => Self::QuestComplete,
            ProjectionEvent::MonsterDeath { .. } => Self::MonsterDeath,
            ProjectionEvent::MonsterRespawn { .. } => Self::MonsterRespawn,
            ProjectionEvent::PlayerDeath { .. } => Self::PlayerDeath,
            ProjectionEvent::PlayersChanged => Self::PlayersChanged,
            ProjectionEvent::AuraAdded { .. } => Self::AuraAdded,
            ProjectionEvent::AuraUpdated { .. } => Self::AuraUpdated,
            ProjectionEvent::AuraRemoved { .. } => Self::AuraRemoved,
            ProjectionEvent::CounterAttackStart { .. } => Self::CounterAttackStart,
            ProjectionEvent::CounterAttackEnd { .. } => Self::CounterAttackEnd,
            ProjectionEvent::PlayerLocation { .. } => Self::PlayerLocation,
            ProjectionEvent::UpdateMessage { .. } => Self::UpdateMessage,
            ProjectionEvent::Zone { .. } => Self::Zone,
        }
    }
}
