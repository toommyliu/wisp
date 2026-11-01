#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum RoomPolicy {
    Public,
    RandomPrivate,
    Specific { room_number: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ScriptOptions {
    pub restart_after_reconnect: bool,
    pub room_policy: RoomPolicy,
    pub safe_start_stop: bool,
}

impl Default for ScriptOptions {
    fn default() -> Self {
        Self {
            restart_after_reconnect: false,
            room_policy: RoomPolicy::RandomPrivate,
            safe_start_stop: true,
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct ScriptOptionsPatch {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub restart_after_reconnect: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub room_policy: Option<RoomPolicy>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub safe_start_stop: Option<bool>,
}

fn present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

impl From<ScriptOptions> for ScriptOptionsPatch {
    fn from(options: ScriptOptions) -> Self {
        Self {
            restart_after_reconnect: Some(options.restart_after_reconnect),
            room_policy: Some(options.room_policy),
            safe_start_stop: Some(options.safe_start_stop),
        }
    }
}
