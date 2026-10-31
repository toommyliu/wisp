use std::time::{Duration, Instant};

use crate::ScriptLink;
use crate::bridge::{Error, Pending, ScriptEvent};
use lucent_protocol::bridge::{Value, from_json, to_json};
use lucent_protocol::command::Command;
use lucent_protocol::event::ProjectionEvent;
use lucent_protocol::query::Query;
use serde::Serialize;

use crate::scripting::RuntimeError;
use crate::scripting::interrupt::Interrupt;

const HOST_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_SLICE: Duration = Duration::from_millis(20);

pub(crate) fn swf(
    link: &ScriptLink,
    interrupt: &Interrupt,
    method: String,
    args: &str,
) -> Result<Option<String>, String> {
    let args: Vec<serde_json::Value> =
        serde_json::from_str(args).map_err(|error| error.to_string())?;
    let value = wait(
        link.call_loader(method, args.into_iter().map(from_json).collect()),
        interrupt,
    )?;
    Ok(to_json(value).map(|value| value.to_string()))
}

pub(crate) fn operate(link: &ScriptLink, id: &str, json: &str) -> Result<(), String> {
    let id = id
        .parse()
        .map_err(|_| "invalid operation handle".to_string())?;
    let operation = serde_json::from_str(json).map_err(|error| format!("{error}"))?;
    link.submit_operation(id, operation)
        .map_err(|error| error.to_string())
}

pub(crate) fn acquire_consumable_claim(link: &ScriptLink, json: &str) -> Result<String, String> {
    let item: Option<i64> = serde_json::from_str(json).map_err(|error| error.to_string())?;
    if item.is_some_and(|id| !(1..=9_007_199_254_740_991).contains(&id)) {
        return Err("invalid consumable item ID".into());
    }
    link.acquire_consumable_claim(item)
        .map(|grant| {
            grant
                .map(
                    |grant| serde_json::json!({ "id": grant.id.to_string(), "first": grant.first }),
                )
                .into()
        })
        .map(|value: serde_json::Value| value.to_string())
        .map_err(|error| error.to_string())
}

pub(crate) fn command(
    link: &ScriptLink,
    interrupt: &Interrupt,
    json: &str,
) -> Result<String, String> {
    let command: Command = serde_json::from_str(json).map_err(|error| error.to_string())?;
    wait(link.command(command), interrupt)
}

pub(crate) fn read(
    link: &ScriptLink,
    interrupt: &Interrupt,
    query: &str,
) -> Result<String, String> {
    let query: Query = serde_json::from_str(query).map_err(|error| error.to_string())?;
    interrupt.block();
    let mut query = Some(query);
    let deadline = Instant::now() + HOST_TIMEOUT;
    let result = loop {
        if let Some(stop) = interrupt.stop() {
            break Err(RuntimeError::from(stop).to_string());
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break Err(Error::Timeout.to_string());
        }
        match link.query_for(left.min(STOP_SLICE), &mut query) {
            Err(Error::Timeout) => {}
            result => break result.map_err(|error| error.to_string()),
        }
    };
    interrupt.unblock();
    result
}

fn wait<T>(pending: Pending<T>, interrupt: &Interrupt) -> Result<T, String> {
    interrupt.block();
    let deadline = Instant::now() + HOST_TIMEOUT;
    let result = loop {
        if let Some(stop) = interrupt.stop() {
            break Err(RuntimeError::from(stop).to_string());
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break Err(Error::Timeout.to_string());
        }
        match pending.recv_timeout(left.min(STOP_SLICE)) {
            Err(Error::Timeout) => {}
            result => break result.map_err(|error| error.to_string()),
        }
    };
    interrupt.unblock();
    result
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum Item {
    EventsLost,
    JobPrepared {
        id: String,
        label: String,
        #[serde(rename = "classNames")]
        class_names: Vec<String>,
        warnings: Vec<String>,
    },
    JobFailed {
        id: String,
        error: String,
    },
    OperationResponsibility {
        id: String,
        responsible: bool,
    },
    OperationStarted {
        id: String,
    },
    OperationIssued {
        id: String,
    },
    AutoZone {
        config: lucent_protocol::auto_zone::AutoZoneConfig,
    },
    Loader {
        name: String,
        args: serde_json::Value,
        projected: Vec<ProjectionEvent>,
    },
    Invalidated,
    Operation {
        id: String,
        answer: Option<lucent_protocol::operation::OperationAnswer>,
        error: Option<String>,
    },
}

impl From<ScriptEvent> for Item {
    fn from(event: ScriptEvent) -> Self {
        match event {
            ScriptEvent::JobPrepared {
                id,
                label,
                class_names,
                warnings,
            } => Item::JobPrepared {
                id: id.to_string(),
                label,
                class_names,
                warnings,
            },
            ScriptEvent::JobFailed { id, error } => Item::JobFailed {
                id: id.to_string(),
                error,
            },
            ScriptEvent::ProfileDeath { .. } => {
                unreachable!("profile deaths only reach the controller")
            }
            ScriptEvent::OperationResponsibility { id, responsible } => {
                Item::OperationResponsibility {
                    id: id.to_string(),
                    responsible,
                }
            }
            ScriptEvent::OperationStarted { id } => Item::OperationStarted { id: id.to_string() },
            ScriptEvent::OperationIssued { id } => Item::OperationIssued { id: id.to_string() },
            ScriptEvent::AutoZoneChanged { config } => Item::AutoZone { config },
            ScriptEvent::Loader { event, projected } => Item::Loader {
                name: event.name,
                args: to_json(Value::List(event.args)).unwrap_or_default(),
                projected,
            },
            ScriptEvent::Invalidated => Item::Invalidated,
            ScriptEvent::Operation { id, answer } => match answer {
                Ok(answer) => Item::Operation {
                    id: id.to_string(),
                    answer: Some(answer),
                    error: None,
                },
                Err(error) => Item::Operation {
                    id: id.to_string(),
                    answer: None,
                    error: Some(error.to_string()),
                },
            },
        }
    }
}

#[path = "web.rs"]
pub(crate) mod web;
