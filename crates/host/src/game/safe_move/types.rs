use crate::game::travel;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RoomPolicy {
    Public,
    Specific { room: u32 },
    RandomPrivate { room: u32 },
}
#[derive(Clone, Debug, Deserialize)]
pub struct Snapshot {
    pub logged_in: bool,
    pub username: String,
    pub equipped_house: bool,
    pub own_house: bool,
    pub travel: travel::Snapshot,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Travel(travel::Action),
    IsReturnInfoNull,
    GetReturnInfo,
    SetReturnInfo(Value),
    House(String),
}
impl Action {
    pub fn loader_call(&self) -> (&str, Vec<Value>) {
        match self {
            Self::Travel(action) => {
                let (method, args) = action.loader_call();
                (method, args.into_iter().map(Value::String).collect())
            }
            Self::IsReturnInfoNull => ("flash.isNull", vec![json!("world.returnInfo")]),
            Self::GetReturnInfo => ("flash.getGameObject", vec![json!("world.returnInfo")]),
            Self::SetReturnInfo(value) => (
                "flash.setGameObject",
                vec![json!("world.returnInfo"), value.clone()],
            ),
            Self::House(username) => (
                "flash.callGameFunction",
                vec![
                    json!("sfc.sendString"),
                    json!(format!("%xt%zm%house%1%{username}%")),
                ],
            ),
        }
    }
}
#[derive(Debug)]
pub struct Command {
    pub id: u64,
    pub action: Action,
}
#[derive(Debug)]
pub enum Reply {
    Accepted,
    Value(Value),
    Unavailable,
    Travel(travel::Reply),
    Error(String),
}
#[derive(Debug)]
pub enum Event {
    Tick,
    Completed { id: u64, reply: Reply },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Skipped,
    Aborted,
    Moved,
    TimedOut,
    Unavailable,
    Error(String),
}
#[derive(Default, Debug)]
pub struct Effects {
    pub command: Option<Command>,
    pub next_deadline: Option<u64>,
    pub outcome: Option<Outcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    InvalidRoom,
}
