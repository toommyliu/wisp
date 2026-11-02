use super::{AttachmentId, claims::ClaimResource};
use crate::{
    bridge::{Error, Value},
    projection::wire::{wire_bool, wire_i64},
};
use lucent_protocol::{Packet, PacketEncoding};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Resource {
    Quest(i64),
    InventoryUse(i64),
}
impl Resource {
    pub(crate) fn claim(self) -> ClaimResource {
        match self {
            Self::Quest(id) => ClaimResource::Quest { id },
            Self::InventoryUse(item) => ClaimResource::InventoryUse { item },
        }
    }
    pub(crate) fn from_claim(resource: ClaimResource) -> Option<Self> {
        match resource {
            ClaimResource::Quest { id } => Some(Self::Quest(id)),
            ClaimResource::InventoryUse { item } => Some(Self::InventoryUse(item)),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Action {
    Accept(i64),
    Complete(i64),
    Abandon(i64),
    Use(i64),
}
impl Action {
    pub(crate) fn parse(name: &str, args: &[Value]) -> Option<Self> {
        fn id(value: &Value) -> Option<i64> {
            match value {
                Value::Number(n)
                    if n.is_finite()
                        && n.fract() == 0.
                        && *n > 0.
                        && *n <= 9_007_199_254_740_991. =>
                {
                    Some(*n as i64)
                }
                _ => None,
            }
        }
        match name {
            "quests.accept" => Some(Self::Accept(id(args.first()?)?)),
            "quests.complete" => Some(Self::Complete(id(args.first()?)?)),
            "quests.abandon" => Some(Self::Abandon(id(args.first()?)?)),
            "inventory.use" => {
                let Value::Object(selector) = args.first()? else {
                    return None;
                };
                Some(Self::Use(id(selector.get("itemId")?)?))
            }
            _ => None,
        }
    }
    pub(crate) fn resource(self) -> Resource {
        match self {
            Self::Accept(id) | Self::Complete(id) | Self::Abandon(id) => Resource::Quest(id),
            Self::Use(id) => Resource::InventoryUse(id),
        }
    }
    fn boolean(self) -> bool {
        matches!(self, Self::Accept(_) | Self::Use(_))
    }
}
enum Phase {
    Calling,
    Waiting(Instant),
}
struct Issued {
    id: u64,
    owner: AttachmentId,
    action: Action,
    phase: Phase,
}
#[derive(Default)]
pub(crate) struct Actions {
    next: u64,
    pending: HashMap<Resource, Issued>,
}
impl Actions {
    pub(crate) fn busy(&mut self, resource: Resource, now: Instant) -> bool {
        self.pending.retain(|_, issued| match issued.phase {
            Phase::Calling => true,
            Phase::Waiting(until) => now < until,
        });
        self.pending.contains_key(&resource)
    }
    pub(crate) fn issue(
        &mut self,
        owner: AttachmentId,
        action: Action,
        now: Instant,
    ) -> Result<u64, Error> {
        let resource = action.resource();
        if self.busy(resource, now) {
            return Err(busy());
        }
        if self.pending.len() >= crate::bridge::EVENT_CAPACITY {
            return Err(Error::Overflow);
        }
        self.next = self.next.checked_add(1).ok_or(Error::Overflow)?;
        self.pending.insert(
            resource,
            Issued {
                id: self.next,
                owner,
                action,
                phase: Phase::Calling,
            },
        );
        Ok(self.next)
    }
    pub(crate) fn returned(
        &mut self,
        resource: Resource,
        id: u64,
        result: &Result<Value, Error>,
        now: Instant,
    ) {
        let Some(issued) = self
            .pending
            .get_mut(&resource)
            .filter(|issued| issued.id == id)
        else {
            return;
        };
        let not_issued = matches!(
            result,
            Err(Error::Unavailable(_) | Error::LoaderNotReady { .. } | Error::NotReady)
        ) || (issued.action.boolean() && matches!(result, Ok(Value::Bool(false))));
        if not_issued {
            self.pending.remove(&resource);
        } else {
            issued.phase = Phase::Waiting(now + Duration::from_secs(5));
        }
    }
    pub(crate) fn settle(&mut self, owner: AttachmentId, resource: Resource) {
        if self
            .pending
            .get(&resource)
            .is_some_and(|issued| issued.owner == owner)
        {
            self.pending.remove(&resource);
        }
    }
    pub(crate) fn packet(&mut self, packet: &Packet) {
        let Packet::Extension {
            command,
            data,
            encoding: PacketEncoding::Json,
            ..
        } = packet
        else {
            return;
        };
        if command != "ccqr" || data.get("bSuccess").and_then(wire_bool).is_none() {
            return;
        }
        let Some(id) = data.get("QuestID").and_then(wire_i64).filter(|id| *id > 0) else {
            return;
        };
        let resource = Resource::Quest(id);
        if self
            .pending
            .get(&resource)
            .is_some_and(|issued| matches!(issued.action, Action::Complete(_)))
        {
            self.pending.remove(&resource);
        }
    }
    pub(crate) fn clear(&mut self) {
        self.pending.clear();
    }
}
pub(crate) fn busy() -> Error {
    Error::CommandFailed {
        operation: "game action".into(),
        reason: "the quest or item has an unconfirmed action".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn issued_actions_remain_reserved_until_receipt_or_five_seconds_after_return() {
        for receipt in [false, true] {
            let now = Instant::now();
            let mut actions = Actions::default();
            let owner = AttachmentId(1);
            let resource = Resource::Quest(802);
            let id = actions.issue(owner, Action::Complete(802), now).unwrap();
            assert!(actions.busy(resource, now + Duration::from_secs(20)));
            actions.returned(
                resource,
                id,
                &Ok(Value::Null),
                now + Duration::from_secs(20),
            );
            assert!(actions.busy(resource, now + Duration::from_millis(24_999)));
            if receipt {
                actions.packet(&Packet::Extension {
                    command: "ccqr".into(),
                    data: serde_json::json!({"QuestID":802,"bSuccess":true}),
                    raw: String::new(),
                    encoding: PacketEncoding::Json,
                });
                assert!(!actions.busy(resource, now + Duration::from_millis(24_999)));
            }
            assert!(!actions.busy(resource, now + Duration::from_secs(25)));
        }
    }
    #[test]
    fn rejected_boolean_actions_release_immediately_and_unrelated_resources_do_not_conflict() {
        let now = Instant::now();
        let mut actions = Actions::default();
        let owner = AttachmentId(1);
        let id = actions.issue(owner, Action::Accept(802), now).unwrap();
        assert!(
            actions
                .issue(AttachmentId(2), Action::Complete(802), now)
                .is_err()
        );
        assert!(actions.issue(owner, Action::Use(802), now).is_ok());
        assert!(actions.issue(owner, Action::Complete(803), now).is_ok());
        actions.returned(Resource::Quest(802), id, &Ok(Value::Bool(false)), now);
        assert!(!actions.busy(Resource::Quest(802), now));
    }
    #[test]
    fn late_returns_after_invalidation_do_not_modify_new_actions() {
        let now = Instant::now();
        let mut actions = Actions::default();
        let owner = AttachmentId(1);
        let resource = Resource::Quest(802);
        let old = actions.issue(owner, Action::Complete(802), now).unwrap();
        actions.clear();
        let new = actions.issue(owner, Action::Complete(802), now).unwrap();
        actions.returned(resource, old, &Err(Error::Unavailable("old".into())), now);
        assert!(actions.busy(resource, now + Duration::from_secs(20)));
        actions.returned(
            resource,
            new,
            &Ok(Value::Null),
            now + Duration::from_secs(20),
        );
        actions.settle(AttachmentId(2), resource);
        assert!(actions.busy(resource, now + Duration::from_secs(21)));
        actions.settle(owner, resource);
        assert!(!actions.busy(resource, now + Duration::from_secs(21)));
    }
}
