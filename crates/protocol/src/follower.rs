use crate::combat_profile::CombatProfile;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(untagged)]
pub enum FollowerAttackTarget {
    Id(#[ts(type = "number")] u64),
    Name(String),
}

impl FollowerAttackTarget {
    fn normalized(self) -> Option<Self> {
        match self {
            Self::Id(0) => None,
            Self::Id(id) => Some(Self::Id(id.min(MAX_SAFE_INTEGER))),
            Self::Name(name) => {
                let name = name.trim();
                if name.is_empty() {
                    return None;
                }
                let id = name
                    .get(..2)
                    .filter(|prefix| prefix.eq_ignore_ascii_case("id"))
                    .and_then(|_| name[2..].strip_prefix(['.', ':', '\'', '-']))
                    .filter(|digits| {
                        digits.starts_with(|c: char| ('1'..='9').contains(&c))
                            && digits.bytes().all(|byte| byte.is_ascii_digit())
                    });
                Some(match id {
                    Some(digits) => {
                        Self::Id(digits.parse().unwrap_or(u64::MAX).min(MAX_SAFE_INTEGER))
                    }
                    None => Self::Name(name.into()),
                })
            }
        }
    }
    fn identity(&self) -> String {
        match self {
            Self::Id(id) => format!("number:{id}"),
            Self::Name(name) => format!("string:{}", name.to_lowercase()),
        }
    }
}

impl std::fmt::Display for FollowerAttackTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id(id) => write!(formatter, "id:{id}"),
            Self::Name(name) => formatter.write_str(name),
        }
    }
}

const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

pub fn parse_follower_attack_priority(input: &str) -> Vec<FollowerAttackTarget> {
    normalize_attack_priority(
        input
            .split(',')
            .map(|name| FollowerAttackTarget::Name(name.into())),
    )
}

fn normalize_attack_priority(
    targets: impl IntoIterator<Item = FollowerAttackTarget>,
) -> Vec<FollowerAttackTarget> {
    let mut seen = HashSet::new();
    targets
        .into_iter()
        .filter_map(FollowerAttackTarget::normalized)
        .filter(|target| seen.insert(target.identity()))
        .collect()
}

pub fn parse_follower_locations(input: &str) -> Vec<String> {
    normalize_locations(input.lines().map(Into::into))
}

fn normalize_locations(locations: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    locations
        .into_iter()
        .map(|location| location.trim().to_owned())
        .filter(|location| !location.is_empty() && seen.insert(location.to_lowercase()))
        .collect()
}

pub const DEFAULT_FOLLOWER_PROFILE_ID: &str = "generic-base";

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FollowerConfig {
    pub target_name: String,
    pub combat_enabled: bool,
    pub copy_walk: bool,
    pub retry_enabled: bool,
    pub max_attempts: u32,
    pub selected_profile_id: String,
    pub attack_priority: Vec<FollowerAttackTarget>,
    pub locked_zone_fallbacks: Vec<String>,
    pub locked_zone_room_override: String,
}
impl Default for FollowerConfig {
    fn default() -> Self {
        Self {
            target_name: String::new(),
            combat_enabled: true,
            copy_walk: false,
            retry_enabled: true,
            max_attempts: 3,
            selected_profile_id: DEFAULT_FOLLOWER_PROFILE_ID.into(),
            attack_priority: Vec::new(),
            locked_zone_fallbacks: Vec::new(),
            locked_zone_room_override: String::new(),
        }
    }
}
impl FollowerConfig {
    pub fn normalized(self) -> Self {
        let selected_profile_id = self.selected_profile_id.trim();
        Self {
            target_name: self.target_name.trim().to_lowercase(),
            max_attempts: self.max_attempts.max(1),
            selected_profile_id: if selected_profile_id.is_empty() {
                DEFAULT_FOLLOWER_PROFILE_ID.into()
            } else {
                selected_profile_id.into()
            },
            attack_priority: normalize_attack_priority(self.attack_priority),
            locked_zone_fallbacks: normalize_locations(self.locked_zone_fallbacks),
            locked_zone_room_override: self.locked_zone_room_override.trim().into(),
            ..self
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum FollowerPhase {
    #[default]
    Idle,
    Starting,
    Following,
    Walking,
    Combat,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FollowerStatus {
    pub enabled: bool,
    pub running: bool,
    pub phase: FollowerPhase,
    pub attempts_remaining: u32,
    pub last_error: Option<String>,
    pub stopped_reason: Option<String>,
    pub warnings: Vec<String>,
}
impl Default for FollowerStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            running: false,
            phase: FollowerPhase::Idle,
            attempts_remaining: 3,
            last_error: None,
            stopped_reason: None,
            warnings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
pub struct FollowerRecords {
    pub config: FollowerConfig,
    pub profile: Option<CombatProfile>,
    pub status: FollowerStatus,
}

#[cfg(test)]
mod tests {
    use super::*;
    use FollowerAttackTarget::{Id, Name};

    #[test]
    fn id_tokens_become_monster_map_ids_and_merge_their_spellings() {
        assert_eq!(
            parse_follower_attack_priority("id:17, ID.17, id-17, id'17, Defense Drone"),
            [Id(17), Name("Defense Drone".into())]
        );
    }

    #[test]
    fn bare_numbers_stay_monster_names() {
        let config = FollowerConfig {
            attack_priority: vec![
                Name("17".into()),
                Id(17),
                Name("0".into()),
                Name("id:0".into()),
                Id(0),
                Name(" ".into()),
                Name("frogzard".into()),
                Name("FROGZARD".into()),
            ],
            ..FollowerConfig::default()
        }
        .normalized();
        assert_eq!(
            config.attack_priority,
            [
                Name("17".into()),
                Id(17),
                Name("0".into()),
                Name("id:0".into()),
                Name("frogzard".into()),
            ]
        );
    }

    #[test]
    fn formatted_targets_parse_back_to_the_same_targets() {
        let targets = [Id(17), Name("Defense Drone".into())];
        let text = targets
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        assert_eq!(parse_follower_attack_priority(&text), targets);
    }

    #[test]
    fn attack_targets_keep_the_numbers_and_strings_of_the_saved_config() {
        let config: FollowerConfig = serde_json::from_value(serde_json::json!({
            "targetName": " Player-B ",
            "combatEnabled": false,
            "copyWalk": true,
            "retryEnabled": true,
            "maxAttempts": 0,
            "selectedProfileId": "  ",
            "attackPriority": [17, "Defense Drone"],
            "lockedZoneFallbacks": [" doom ", "DOOM", "", "dage"],
            "lockedZoneRoomOverride": " 42 ",
        }))
        .unwrap();
        let config = config.normalized();
        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            serde_json::json!({
                "targetName": "player-b",
                "combatEnabled": false,
                "copyWalk": true,
                "retryEnabled": true,
                "maxAttempts": 1,
                "selectedProfileId": "generic-base",
                "attackPriority": [17, "Defense Drone"],
                "lockedZoneFallbacks": ["doom", "dage"],
                "lockedZoneRoomOverride": "42",
            })
        );
        assert_eq!(
            parse_follower_locations("doom\r\n Doom \n\ndage"),
            ["doom", "dage"]
        );
    }
}
