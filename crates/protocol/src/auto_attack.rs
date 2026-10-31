use crate::combat_profile::CombatProfile;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoAttackConfig {
    pub enabled: bool,
    pub profile: Option<CombatProfile>,
    pub target_priority: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoAttackStatus {
    pub running: bool,
    pub class_names: Vec<String>,
    pub warnings: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
pub struct AutoAttackRecords {
    pub config: AutoAttackConfig,
    pub status: AutoAttackStatus,
}
