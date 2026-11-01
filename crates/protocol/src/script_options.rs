#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum RoomPolicy {
    Public,
    RandomPrivate,
    Specific {
        #[serde(deserialize_with = "room_number")]
        room_number: u32,
    },
}

fn room_number<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let number = <f64 as serde::Deserialize>::deserialize(deserializer)?;
    if number.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&number) {
        Ok(number as u32)
    } else {
        Err(serde::de::Error::custom("expected an unsigned integer"))
    }
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
#[serde(
    rename_all = "camelCase",
    try_from = "serde_json::Map<String, serde_json::Value>"
)]
#[ts(optional_fields)]
pub struct ScriptOptionsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_after_reconnect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_policy: Option<RoomPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_start_stop: Option<bool>,
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for ScriptOptionsPatch {
    type Error = serde_json::Error;

    fn try_from(
        mut fields: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Self, Self::Error> {
        fn field<T: serde::de::DeserializeOwned>(
            value: Option<serde_json::Value>,
        ) -> Result<Option<T>, serde_json::Error> {
            value.map(serde_json::from_value).transpose()
        }
        Ok(Self {
            restart_after_reconnect: field(fields.remove("restartAfterReconnect"))?,
            room_policy: field(fields.remove("roomPolicy"))?,
            safe_start_stop: field(fields.remove("safeStartStop"))?,
        })
    }
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
