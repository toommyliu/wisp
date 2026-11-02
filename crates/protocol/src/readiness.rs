use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub enum ReadinessKey {
    HouseInventory,
    Inventory,
    Map,
    Player,
}

impl ReadinessKey {
    pub const ALL: [Self; 4] = [
        Self::HouseInventory,
        Self::Inventory,
        Self::Map,
        Self::Player,
    ];
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessFlags {
    pub house_inventory: bool,
    pub inventory: bool,
    pub map: bool,
    pub player: bool,
}

impl ReadinessFlags {
    pub fn get_mut(&mut self, key: ReadinessKey) -> &mut bool {
        match key {
            ReadinessKey::HouseInventory => &mut self.house_inventory,
            ReadinessKey::Inventory => &mut self.inventory,
            ReadinessKey::Map => &mut self.map,
            ReadinessKey::Player => &mut self.player,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
pub struct ReadinessSnapshot {
    pub epoch: u64,
    #[serde(rename = "canAct")]
    pub can_act: bool,
    pub failures: BTreeMap<ReadinessKey, String>,
    pub missing: Vec<ReadinessKey>,
    pub state: ReadinessFlags,
}
