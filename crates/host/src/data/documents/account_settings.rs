use lucent_protocol::script_options::{RoomPolicy, ScriptOptions, ScriptOptionsPatch};
use std::ops::RangeInclusive;

use super::json::{self, JsMap, Json, Output};
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
    json::stringify(&Output::Object(vec![
        ("version", Output::Integer(VERSION)),
        ("scripts", Output::Object(fields(options))),
    ]))
}

fn fields(options: &ScriptOptions) -> Vec<(&'static str, Output<'static>)> {
    let room_policy = match options.room_policy {
        RoomPolicy::Public => vec![("kind", Output::String("public"))],
        RoomPolicy::RandomPrivate => vec![("kind", Output::String("random-private"))],
        RoomPolicy::Specific { room_number } => vec![
            ("kind", Output::String("specific")),
            ("roomNumber", Output::Integer(room_number)),
        ],
    };
    let values = [
        Output::Bool(options.restart_after_reconnect),
        Output::Object(room_policy),
        Output::Bool(options.safe_start_stop),
    ];
    SCRIPTS.iter().map(|&(name, _)| name).zip(values).collect()
}

fn from_object(document: &JsMap<Json>) -> Decoded<ScriptOptions> {
    let mut issues = Issues::default();
    let newer = match document.get("version") {
        None => false,
        Some(Json::Number(version)) if version.fract() == 0.0 && *version >= 0.0 => {
            *version > f64::from(VERSION)
        }
        Some(_) => {
            issues.reset.push("version");
            false
        }
    };
    let scripts = match document.get("scripts") {
        Some(Json::Object(scripts)) => Some(scripts),
        None => None,
        Some(_) => {
            issues.reset.push("scripts");
            None
        }
    };
    let field = |name: &str| scripts.and_then(|scripts| scripts.get(name));
    let defaults = ScriptOptions::default();
    let options = ScriptOptions {
        restart_after_reconnect: boolean(field("restartAfterReconnect"))
            .unwrap_or(defaults.restart_after_reconnect),
        room_policy: field("roomPolicy")
            .and_then(room_policy)
            .unwrap_or(defaults.room_policy),
        safe_start_stop: boolean(field("safeStartStop")).unwrap_or(defaults.safe_start_stop),
    };
    for (&(name, path), (_, kept)) in SCRIPTS.iter().zip(fields(&options)) {
        if field(name).is_some_and(|raw| !raw.equals(&kept)) {
            issues.reset.push(path);
        }
    }
    Decoded {
        value: options,
        issues: if newer { Issues::default() } else { issues },
    }
}

fn boolean(raw: Option<&Json>) -> Option<bool> {
    match raw {
        Some(Json::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn room_policy(raw: &Json) -> Option<RoomPolicy> {
    let Json::Object(policy) = raw else {
        return None;
    };
    let Some(Json::String(kind)) = policy.get("kind") else {
        return None;
    };
    match kind.as_str() {
        "public" => Some(RoomPolicy::Public),
        "random-private" => Some(RoomPolicy::RandomPrivate),
        "specific" => match policy.get("roomNumber") {
            Some(Json::Number(room)) if room.fract() == 0.0 && ROOMS.contains(&(*room as u32)) => {
                Some(RoomPolicy::Specific {
                    room_number: *room as u32,
                })
            }
            _ => None,
        },
        _ => None,
    }
}
