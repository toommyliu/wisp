use crate::query::ItemSelector;

#[derive(Debug, serde::Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Operation {
    AcceptDrop { selector: ItemSelector },
    LoadQuests { ids: Vec<f64>, silent: bool },
    LoadBank { force: bool },
    OpenBank { force: bool },
    EquipConsumable { selector: ItemSelector },
    PrepareConsumableClaim { claim: String },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum OperationAnswer {
    AcceptDrop(bool),
    LoadQuests(Vec<bool>),
    LoadBank(bool),
    OpenBank(bool),
    EquipConsumable(bool),
}
