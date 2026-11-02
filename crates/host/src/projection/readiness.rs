use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Avatar {
    #[default]
    Unknown,
    Loading,
    Loaded,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerReadiness {
    pub epoch: u64,
    pub connected: bool,
    pub(crate) logged_out: bool,
    pub facts: bool,
    pub avatar: Avatar,
}

impl PlayerReadiness {
    pub fn can_act(self) -> bool {
        self.connected && self.facts && self.avatar == Avatar::Loaded
    }

    pub(crate) fn needs_confirmation(self) -> bool {
        self.connected && self.facts && self.avatar != Avatar::Loaded
    }
}

pub use lucent_protocol::readiness::{ReadinessFlags, ReadinessKey, ReadinessSnapshot};

#[derive(Debug, Default)]
pub struct ReadinessReceipts {
    completed: ReadinessFlags,
    failures: BTreeMap<ReadinessKey, String>,
    epoch: u64,
}

impl ReadinessReceipts {
    pub fn complete(&mut self, key: ReadinessKey) {
        *self.completed.get_mut(key) = true;
        self.failures.remove(&key);
    }

    pub fn fail(&mut self, key: ReadinessKey, reason: String) {
        *self.completed.get_mut(key) = false;
        self.failures.insert(key, reason);
    }

    pub fn reset(&mut self) {
        self.completed = ReadinessFlags::default();
        self.failures.clear();
        self.epoch += 1;
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn failures(&self) -> &BTreeMap<ReadinessKey, String> {
        &self.failures
    }

    pub fn inspect(
        &self,
        house_hydrated: bool,
        inventory_hydrated: bool,
        map_name: &str,
        self_bound: bool,
    ) -> ReadinessSnapshot {
        let mut state = ReadinessFlags {
            house_inventory: self.completed.house_inventory && house_hydrated,
            inventory: self.completed.inventory && inventory_hydrated,
            map: self.completed.map && !map_name.is_empty(),
            player: self.completed.player && self_bound,
        };
        let missing = ReadinessKey::ALL
            .into_iter()
            .filter(|key| !*state.get_mut(*key))
            .collect();
        ReadinessSnapshot {
            epoch: self.epoch,
            can_act: false,
            failures: self.failures.clone(),
            missing,
            state,
        }
    }
}
