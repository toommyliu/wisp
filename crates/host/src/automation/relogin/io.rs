use super::{REPLY_TIMEOUT, ReloginHost};
use crate::bridge::{Error, Pending, Value};
use lucent_protocol::command::Command;
use std::{task::Waker, time::Instant};

#[derive(Clone, serde::Serialize)]
pub(crate) struct Snapshot {
    pub username: String,
    pub password: String,
    pub connected: Option<bool>,
    pub logged_in: bool,
    pub ready: bool,
    pub servers: Vec<String>,
}
pub(super) struct Wait<T> {
    pub(super) reply: Pending<T>,
    pub(super) deadline: Instant,
}
impl<T> Wait<T> {
    pub(super) fn new(reply: Pending<T>, waker: &Waker, now: Instant) -> Self {
        reply.set_waker(waker.clone());
        Self {
            reply,
            deadline: now + REPLY_TIMEOUT,
        }
    }
    pub(super) fn poll(&self, now: Instant) -> Option<Result<T, Error>> {
        self.reply
            .try_recv()
            .or_else(|| (now >= self.deadline).then_some(Err(Error::Timeout)))
    }
}
pub(super) enum Slot<T> {
    Waiting(Wait<T>),
    Done(Result<T, Error>),
}
impl<T> Slot<T> {
    pub(super) fn new(reply: Pending<T>, waker: &Waker, now: Instant) -> Self {
        Self::Waiting(Wait::new(reply, waker, now))
    }
    pub(super) fn poll(&mut self, now: Instant) {
        if let Self::Waiting(wait) = self
            && let Some(value) = wait.poll(now)
        {
            *self = Self::Done(value);
        }
    }
    pub(super) fn done(&self) -> bool {
        matches!(self, Self::Done(_))
    }
    pub(super) fn value(&self) -> Option<&T> {
        if let Self::Done(Ok(value)) = self {
            Some(value)
        } else {
            None
        }
    }
}
pub(super) struct Observation {
    snapshot: Slot<Snapshot>,
    pub(super) select: Slot<Value>,
    pub(super) deadline: Instant,
}
#[derive(Default)]
pub(super) struct Observed {
    pub(super) connected: Option<bool>,
    pub(super) ready: bool,
    pub(super) logged_in: bool,
    pub(super) select: bool,
}
impl Observation {
    pub(super) fn new(host: &impl ReloginHost, waker: &Waker, now: Instant) -> Self {
        Self {
            snapshot: Slot::new(host.snapshot(), waker, now),
            select: Slot::new(
                host.call(
                    "flash.isNull",
                    vec![Value::String("mcLogin.sl.iList".into())],
                ),
                waker,
                now,
            ),
            deadline: now + REPLY_TIMEOUT,
        }
    }
    pub(super) fn poll(&mut self, now: Instant) -> Option<Observed> {
        self.snapshot.poll(now);
        self.select.poll(now);
        if !self.snapshot.done() || !self.select.done() {
            return None;
        }
        let snapshot = self.snapshot.value();
        Some(Observed {
            connected: snapshot.and_then(|s| s.connected),
            logged_in: snapshot.is_some_and(|s| s.logged_in),
            ready: snapshot.is_some_and(|s| s.ready),
            select: self.select.value() == Some(&Value::Bool(false)),
        })
    }
}
pub(super) struct Capture {
    fresh: Slot<String>,
    cached: Slot<Snapshot>,
}
impl Capture {
    pub(super) fn new(host: &impl ReloginHost, waker: &Waker, now: Instant) -> Self {
        Self {
            fresh: Slot::new(host.command(Command::RefreshCredentials {}), waker, now),
            cached: Slot::new(host.snapshot(), waker, now),
        }
    }
    pub(super) fn poll(&mut self, now: Instant) -> Option<Result<(String, String, bool), Error>> {
        self.fresh.poll(now);
        self.cached.poll(now);
        if !self.fresh.done() || !self.cached.done() {
            return None;
        }
        #[derive(serde::Deserialize)]
        struct Credentials {
            username: String,
            password: String,
        }
        let fresh = match self
            .fresh
            .value()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        {
            Some(value) => match serde_json::from_value::<Credentials>(value) {
                Ok(credentials) => Some(credentials),
                Err(error) => {
                    return Some(Err(Error::CommandDecode {
                        callback: "refreshCredentials".into(),
                        reason: error.to_string(),
                    }));
                }
            },
            None => None,
        };
        let cached = self.cached.value();
        Some(match fresh {
            Some(value) => Ok((
                value.username.trim().into(),
                value.password,
                cached.is_some_and(|s| s.ready && s.logged_in),
            )),
            None => cached
                .map(|s| {
                    (
                        s.username.trim().into(),
                        s.password.clone(),
                        s.ready && s.logged_in,
                    )
                })
                .ok_or(Error::ReadPanicked),
        })
    }
}
pub(super) struct ServerList {
    pub(super) deadline: Instant,
    fresh: Slot<String>,
    cached: Slot<Snapshot>,
}
impl ServerList {
    pub(super) fn new(host: &impl ReloginHost, waker: &Waker, now: Instant) -> Self {
        Self {
            deadline: now + REPLY_TIMEOUT,
            fresh: Slot::new(host.command(Command::RefreshServers {}), waker, now),
            cached: Slot::new(host.snapshot(), waker, now),
        }
    }
    pub(super) fn poll(&mut self, now: Instant) -> Option<Result<Vec<String>, Error>> {
        self.fresh.poll(now);
        self.cached.poll(now);
        if !self.fresh.done() || !self.cached.done() {
            return None;
        }
        #[derive(serde::Deserialize)]
        struct Server {
            name: String,
        }
        let fresh = self
            .fresh
            .value()
            .and_then(|json| serde_json::from_str::<Vec<Server>>(json).ok());
        Some(
            fresh
                .map(|servers| servers.into_iter().map(|s| s.name).collect())
                .or_else(|| self.cached.value().map(|s| s.servers.clone()))
                .ok_or(Error::ReadPanicked),
        )
    }
}
#[derive(serde::Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum SelectionStatus {
    Selected,
    Blocked,
    NotFound,
    NotReady,
}
#[derive(serde::Deserialize)]
pub(super) struct Selection {
    pub(super) ok: bool,
    pub(super) status: SelectionStatus,
    pub(super) message: String,
    pub(super) reason: Option<String>,
}
pub(super) fn selection(value: Value) -> Option<Selection> {
    let selection: Selection = serde_json::from_value(loader_json(value)?).ok()?;
    (selection.ok == (selection.status == SelectionStatus::Selected)).then_some(selection)
}
pub(super) fn loader_json(value: Value) -> Option<serde_json::Value> {
    match value {
        Value::String(s) => serde_json::from_str(&s).ok(),
        v => lucent_protocol::bridge::to_json(v),
    }
}
