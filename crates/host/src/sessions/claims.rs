use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use super::AttachmentId;
use crate::bridge::{EVENT_CAPACITY, Error};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Owner {
    pub(crate) attachment: AttachmentId,
    pub(crate) job: Option<crate::game::jobs::JobId>,
}

impl Owner {
    pub(crate) fn script(attachment: AttachmentId) -> Self {
        Self {
            attachment,
            job: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    player: u64,
    connection: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClaimResource {
    ConsumableSlot5 { item: Option<i64> },
    Quest { id: i64 },
    InventoryUse { item: i64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Preparation {
    Unchecked,
    EquipRequired,
    Ready,
}

struct HeldClaim {
    owner: Owner,
    binding: Binding,
    resource: ClaimResource,
}

pub struct ClaimGrant {
    pub id: u64,
    pub first: bool,
}

pub(crate) struct Claims {
    binding: Binding,
    holders: HashMap<u64, HeldClaim>,
    preparation: Preparation,
    next: u64,
}

impl Default for Claims {
    fn default() -> Self {
        static NEXT_PLAYER: AtomicU64 = AtomicU64::new(1);
        Self {
            binding: Binding {
                player: NEXT_PLAYER.fetch_add(1, Ordering::Relaxed),
                connection: 0,
            },
            holders: HashMap::new(),
            preparation: Preparation::Unchecked,
            next: 0,
        }
    }
}

impl Claims {
    pub(crate) fn binding(&self) -> Binding {
        self.binding
    }

    pub(crate) fn acquire(
        &mut self,
        owner: Owner,
        resource: ClaimResource,
    ) -> Result<Option<ClaimGrant>, Error> {
        let conflicts = |held: &HeldClaim| match (held.resource, resource) {
            (ClaimResource::Quest { id: a }, ClaimResource::Quest { id: b }) => a == b,
            (ClaimResource::InventoryUse { item: a }, ClaimResource::InventoryUse { item: b }) => {
                a == b
            }
            (
                ClaimResource::ConsumableSlot5 { item: a },
                ClaimResource::ConsumableSlot5 { item: b },
            ) => a != b,
            _ => false,
        };
        if self.holders.values().any(conflicts) {
            return Ok(None);
        }
        if self.holders.len() >= EVENT_CAPACITY {
            return Err(Error::Overflow);
        }
        let first = !self.holders.values().any(|held| held.resource == resource);
        if first && matches!(resource, ClaimResource::ConsumableSlot5 { .. }) {
            self.preparation = Preparation::Unchecked;
        }
        self.next = self.next.checked_add(1).ok_or(Error::Overflow)?;
        self.holders.insert(
            self.next,
            HeldClaim {
                owner,
                binding: self.binding,
                resource,
            },
        );
        Ok(Some(ClaimGrant {
            id: self.next,
            first,
        }))
    }

    pub(crate) fn resource(&self, owner: Owner, id: u64) -> Option<ClaimResource> {
        self.holders
            .get(&id)
            .filter(|held| held.owner == owner && held.binding == self.binding)
            .map(|held| held.resource)
    }
    pub(crate) fn owning(&self, owner: Owner, resource: ClaimResource) -> Option<u64> {
        self.holders
            .iter()
            .find(|(_, held)| {
                held.owner == owner && held.binding == self.binding && held.resource == resource
            })
            .map(|(id, _)| *id)
    }
    pub(crate) fn held_by_other(&self, owner: Owner, resource: ClaimResource) -> bool {
        self.holders.values().any(|held| {
            held.owner != owner && held.binding == self.binding && held.resource == resource
        })
    }

    pub(crate) fn preparation(&self, owner: Owner, id: u64) -> Option<(i64, Binding, Preparation)> {
        let held = self.holders.get(&id)?;
        if held.owner != owner || held.binding != self.binding {
            return None;
        }
        let ClaimResource::ConsumableSlot5 { item: Some(item) } = held.resource else {
            return None;
        };
        Some((item, held.binding, self.preparation))
    }

    pub(crate) fn set_preparation(
        &mut self,
        binding: Binding,
        item: i64,
        preparation: Preparation,
    ) -> bool {
        if binding != self.binding
            || !self
                .holders
                .values()
                .any(|held| held.resource == ClaimResource::ConsumableSlot5 { item: Some(item) })
        {
            return false;
        }
        self.preparation = preparation;
        true
    }

    pub(crate) fn release(&mut self, owner: Owner, id: u64) {
        if self
            .holders
            .get(&id)
            .is_some_and(|held| held.owner == owner && held.binding == self.binding)
        {
            self.holders.remove(&id);
        }
    }

    pub(crate) fn release_owner(&mut self, owner: Owner) {
        self.holders.retain(|_, held| held.owner != owner);
    }

    pub(crate) fn release_attachment(&mut self, attachment: AttachmentId) {
        self.holders
            .retain(|_, held| held.owner.attachment != attachment);
    }

    pub(crate) fn clear(&mut self) {
        self.holders.clear();
        self.preparation = Preparation::Unchecked;
        self.binding.connection += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quests_exclude_other_callers_without_disturbing_consumable_preparation() {
        let mut claims = Claims::default();
        let a = Owner::script(super::super::AttachmentId(1));
        let b = Owner::script(super::super::AttachmentId(2));
        let item = claims
            .acquire(a, ClaimResource::ConsumableSlot5 { item: Some(100) })
            .unwrap()
            .unwrap();
        assert!(claims.set_preparation(claims.binding(), 100, Preparation::Ready));
        let quest = claims
            .acquire(a, ClaimResource::Quest { id: 802 })
            .unwrap()
            .unwrap();
        assert!(
            claims
                .acquire(b, ClaimResource::Quest { id: 802 })
                .unwrap()
                .is_none()
        );
        assert!(
            claims
                .acquire(b, ClaimResource::Quest { id: 803 })
                .unwrap()
                .is_some()
        );
        assert_eq!(
            claims.preparation(a, item.id).unwrap().2,
            Preparation::Ready
        );
        claims.release(b, quest.id);
        assert!(
            claims
                .acquire(b, ClaimResource::Quest { id: 802 })
                .unwrap()
                .is_none()
        );
        claims.release_owner(a);
        assert!(
            claims
                .acquire(b, ClaimResource::Quest { id: 802 })
                .unwrap()
                .is_some()
        );
        claims.clear();
        assert!(
            claims
                .acquire(a, ClaimResource::Quest { id: 803 })
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn shared_preparation_survives_owner_release_and_rejects_stale_or_foreign_holders() {
        let mut claims = Claims::default();
        let a = Owner::script(super::super::AttachmentId(1));
        let b = Owner::script(super::super::AttachmentId(2));
        let resource = ClaimResource::ConsumableSlot5 { item: Some(100) };
        let first = claims.acquire(a, resource).unwrap().unwrap();
        let shared = claims.acquire(b, resource).unwrap().unwrap();
        let binding = claims.binding();
        assert_eq!(claims.preparation(b, first.id), None);
        assert!(claims.set_preparation(binding, 100, Preparation::EquipRequired));
        claims.release_owner(a);
        assert_eq!(
            claims.preparation(b, shared.id),
            Some((100, binding, Preparation::EquipRequired))
        );
        assert!(claims.set_preparation(binding, 100, Preparation::Ready));
        assert_eq!(
            claims.preparation(b, shared.id),
            Some((100, binding, Preparation::Ready))
        );
        claims.release(b, shared.id);
        let next = claims.acquire(a, resource).unwrap().unwrap();
        assert_eq!(
            claims.preparation(a, next.id),
            Some((100, binding, Preparation::Unchecked))
        );
        claims.clear();
        assert_eq!(claims.preparation(a, next.id), None);
        let fresh = claims.acquire(b, resource).unwrap().unwrap();
        assert!(!claims.set_preparation(binding, 100, Preparation::Ready));
        assert_eq!(
            claims.preparation(b, fresh.id),
            Some((100, claims.binding(), Preparation::Unchecked))
        );
    }

    #[test]
    fn compatible_holders_share_until_the_last_release_and_stale_releases_do_nothing() {
        let mut claims = Claims::default();
        let a = Owner::script(super::super::AttachmentId(1));
        let b = Owner::script(super::super::AttachmentId(2));
        let resource = ClaimResource::ConsumableSlot5 { item: Some(100) };
        let first = claims.acquire(a, resource).unwrap().unwrap();
        let shared = claims.acquire(b, resource).unwrap().unwrap();
        assert!(first.first);
        assert!(!shared.first);
        assert!(
            claims
                .acquire(b, ClaimResource::ConsumableSlot5 { item: Some(200) })
                .unwrap()
                .is_none()
        );
        claims.release(b, first.id);
        claims.release(a, first.id);
        assert!(
            claims
                .acquire(a, ClaimResource::ConsumableSlot5 { item: Some(200) })
                .unwrap()
                .is_none()
        );
        claims.release_owner(b);
        let next = claims
            .acquire(b, ClaimResource::ConsumableSlot5 { item: Some(200) })
            .unwrap()
            .unwrap();
        assert!(next.first);
        claims.release(a, first.id);
        claims.release(b, shared.id);
        assert!(claims.acquire(a, resource).unwrap().is_none());
    }

    #[test]
    fn only_the_owner_can_release_a_holder() {
        let mut claims = Claims::default();
        let owner = Owner::script(super::super::AttachmentId(1));
        let other = Owner::script(super::super::AttachmentId(2));
        let first = claims
            .acquire(owner, ClaimResource::ConsumableSlot5 { item: Some(100) })
            .unwrap()
            .unwrap();
        claims.release(other, first.id);
        assert!(
            claims
                .acquire(other, ClaimResource::ConsumableSlot5 { item: Some(200) })
                .unwrap()
                .is_none()
        );
        claims.release(owner, first.id);
        assert!(
            claims
                .acquire(other, ClaimResource::ConsumableSlot5 { item: Some(200) })
                .unwrap()
                .unwrap()
                .first
        );
    }

    #[test]
    fn ambient_holders_share_and_invalidation_frees_the_slot() {
        let mut claims = Claims::default();
        let a = Owner::script(super::super::AttachmentId(1));
        let resource = ClaimResource::ConsumableSlot5 { item: None };
        assert!(claims.acquire(a, resource).unwrap().unwrap().first);
        assert!(!claims.acquire(a, resource).unwrap().unwrap().first);
        assert!(
            claims
                .acquire(a, ClaimResource::ConsumableSlot5 { item: Some(100) })
                .unwrap()
                .is_none()
        );
        claims.clear();
        assert!(
            claims
                .acquire(a, ClaimResource::ConsumableSlot5 { item: Some(100) })
                .unwrap()
                .unwrap()
                .first
        );
    }
}
