use lucent_protocol::script_options::{RoomPolicy, ScriptOptions, ScriptOptionsPatch};
use std::ops::RangeInclusive;

use serde_json::{Map, Value, json};

use super::json;
use super::{Decoded, Issues, Loaded, decode_document, trim};
use crate::accounts::AccountsError;

const VERSION: u32 = 1;

const ROOMS: RangeInclusive<u32> = 1..=99_999;

const SCRIPTS: [(&str, &str); 3] = [
    ("restartAfterReconnect", "scripts.restartAfterReconnect"),
    ("roomPolicy", "scripts.roomPolicy"),
    ("safeStartStop", "scripts.safeStartStop"),
];

pub(in crate::data) fn file_name(username: &str) -> Result<String, AccountsError> {
    let name = trim(username).to_lowercase();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return Err(AccountsError::InvalidUsername);
    }
    Ok(format!("{name}.json"))
}

pub(in crate::data) fn decode(file: Option<&[u8]>) -> Loaded<ScriptOptions> {
    decode_document(file, from_object)
}

pub(in crate::data) fn apply(options: ScriptOptions, patch: ScriptOptionsPatch) -> ScriptOptions {
    let room_policy = match patch.room_policy.unwrap_or(options.room_policy) {
        RoomPolicy::Specific { room_number } if !ROOMS.contains(&room_number) => {
            ScriptOptions::default().room_policy
        }
        room_policy => room_policy,
    };
    ScriptOptions {
        restart_after_reconnect: patch
            .restart_after_reconnect
            .unwrap_or(options.restart_after_reconnect),
        room_policy,
        safe_start_stop: patch.safe_start_stop.unwrap_or(options.safe_start_stop),
    }
}

pub(in crate::data) fn encode(options: &ScriptOptions) -> Vec<u8> {
    json::stringify(&json!({"version": VERSION, "scripts": fields(options)}))
}

fn fields(options: &ScriptOptions) -> Map<String, Value> {
    let room_policy = match options.room_policy {
        RoomPolicy::Public => json!({"kind": "public"}),
        RoomPolicy::RandomPrivate => json!({"kind": "random-private"}),
        RoomPolicy::Specific { room_number } => {
            json!({"kind": "specific", "roomNumber": room_number})
        }
    };
    let values = [
        Value::Bool(options.restart_after_reconnect),
        room_policy,
        Value::Bool(options.safe_start_stop),
    ];
    let names = SCRIPTS.iter().map(|&(name, _)| name.to_owned());
    names.zip(values).collect()
}

fn from_object(document: &Map<String, Value>) -> Decoded<ScriptOptions> {
    let mut issues = Issues::default();
    let newer = match document.get("version").map(integer) {
        None => false,
        Some(Some(version)) if version >= 0.0 => version > f64::from(VERSION),
        Some(_) => {
            issues.reset.push("version");
            false
        }
    };
    let scripts = match document.get("scripts") {
        Some(Value::Object(scripts)) => Some(scripts),
        None => None,
        Some(_) => {
            issues.reset.push("scripts");
            None
        }
    };
    let field = |name: &str| scripts.and_then(|scripts| scripts.get(name));
    let defaults = ScriptOptions::default();
    let options = ScriptOptions {
        restart_after_reconnect: field("restartAfterReconnect")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.restart_after_reconnect),
        room_policy: field("roomPolicy")
            .and_then(room_policy)
            .unwrap_or(defaults.room_policy),
        safe_start_stop: field("safeStartStop")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.safe_start_stop),
    };
    for (&(name, path), kept) in SCRIPTS.iter().zip(fields(&options).values()) {
        if field(name).is_some_and(|raw| !json::same(raw, kept)) {
            issues.reset.push(path);
        }
    }
    Decoded {
        value: options,
        issues: if newer { Issues::default() } else { issues },
    }
}

fn integer(raw: &Value) -> Option<f64> {
    raw.as_f64().filter(|value| value.fract() == 0.0)
}

fn room_policy(raw: &Value) -> Option<RoomPolicy> {
    match raw.get("kind")?.as_str()? {
        "public" => Some(RoomPolicy::Public),
        "random-private" => Some(RoomPolicy::RandomPrivate),
        "specific" => {
            let room = integer(raw.get("roomNumber")?)?;
            ROOMS
                .contains(&(room as u32))
                .then_some(RoomPolicy::Specific {
                    room_number: room as u32,
                })
        }
        _ => None,
    }
}
