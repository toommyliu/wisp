mod boosts;
mod quests;

use crate::{
    ScriptLink,
    bridge::{Error, EventInterests, EventKind, Pending, ScriptEvent, Value},
};
use lucent_protocol::{
    environment::EnvironmentState,
    event::ProjectionEvent,
    item::ItemData,
    operation::{Operation, OperationAnswer},
};
use std::{
    collections::HashSet,
    task::Waker,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);
const QUEST_INTERVAL: Duration = Duration::from_secs(1);
const BOOST_INTERVAL: Duration = Duration::from_secs(5);

pub(crate) struct Lease(Option<Box<dyn FnOnce() + Send>>);
impl Lease {
    pub(crate) fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self(Some(Box::new(release)))
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            release();
        }
    }
}

#[derive(Clone, Default, serde::Serialize)]
pub(crate) struct Snapshot {
    pub config: EnvironmentState,
    pub logged_in: bool,
    pub quests: Vec<f64>,
    pub items: Vec<ItemData>,
}

trait Host {
    fn snapshot(&self, register: bool) -> Pending<Snapshot>;
    fn call(&self, method: &str, args: Vec<Value>) -> Pending<Value>;
    fn load(&self, ids: Vec<f64>) -> Result<u64, Error>;
    fn cancel(&self, id: u64);
    fn claim(&self, id: f64) -> Result<Option<Lease>, Error>;
    fn use_claim(&self, item: i64) -> Result<Option<Lease>, Error>;
}
impl Host for ScriptLink {
    fn snapshot(&self, register: bool) -> Pending<Snapshot> {
        self.environment_snapshot(register)
    }
    fn call(&self, method: &str, args: Vec<Value>) -> Pending<Value> {
        self.call_loader(method, args)
    }
    fn load(&self, ids: Vec<f64>) -> Result<u64, Error> {
        self.operate(Operation::LoadQuests { ids, silent: true })
    }
    fn cancel(&self, id: u64) {
        self.cancel_operation(id);
    }
    fn use_claim(&self, item: i64) -> Result<Option<Lease>, Error> {
        self.inventory_use_lease(item)
    }
    fn claim(&self, id: f64) -> Result<Option<Lease>, Error> {
        self.quest_lease(id as i64)
    }
}

struct Read<T> {
    reply: Pending<T>,
    deadline: Instant,
}
impl<T> Read<T> {
    fn new(reply: Pending<T>, waker: &Waker, now: Instant) -> Self {
        reply.set_waker(waker.clone());
        waker.wake_by_ref();
        Self {
            reply,
            deadline: now + TIMEOUT,
        }
    }
    fn poll(&self, now: Instant) -> Option<Result<T, Error>> {
        self.reply
            .try_recv()
            .or_else(|| (now >= self.deadline).then_some(Err(Error::Timeout)))
    }
}
fn call(
    host: &impl Host,
    waker: &Waker,
    now: Instant,
    method: &str,
    args: Vec<Value>,
) -> Read<Value> {
    Read::new(host.call(method, args), waker, now)
}
fn boolean(value: Value) -> Result<bool, Error> {
    match value {
        Value::Bool(b) => Ok(b),
        _ => Err(Error::CommandDecode {
            callback: "environment".into(),
            reason: "expected boolean".into(),
        }),
    }
}
#[derive(Default)]
struct Failures(HashSet<String>);
impl Failures {
    fn set(&mut self, key: &str, success: bool) {
        if success {
            if self.0.remove(key) {
                tracing::info!(key, "environment automation recovered");
            }
        } else if self.0.insert(key.into()) {
            tracing::warn!(key, "environment automation failed");
        }
    }
    fn clear(&mut self, prefix: &str) {
        self.0.retain(|key| !key.starts_with(prefix));
    }
}

pub struct EnvironmentPolicy {
    link: ScriptLink,
    waker: Waker,
    state: Environment,
    version: u64,
}
impl EnvironmentPolicy {
    pub const INTERESTS: EventInterests = EventInterests::new(&[
        EventKind::Login,
        EventKind::Logout,
        EventKind::Invalidated,
        EventKind::Operation,
        EventKind::QuestComplete,
    ]);
    pub fn new(link: ScriptLink, waker: Waker) -> Self {
        link.set_waker(waker.clone());
        Self {
            link,
            waker,
            state: Environment::new(),
            version: u64::MAX,
        }
    }
    pub fn advance(&mut self, now: Instant) -> Option<Instant> {
        if self.link.player_status().is_stopped() {
            self.state.reset(&self.link);
            return None;
        }
        let version = self.link.records.changes.version();
        if version != self.version {
            self.version = version;
            self.state.dirty = true;
        }
        for index in 0..64 {
            match self.link.try_event() {
                Ok(Some(ScriptEvent::Operation { id, answer })) => {
                    self.state.quests.loaded(id, answer)
                }
                Ok(Some(ScriptEvent::Loader { projected, .. })) => {
                    for event in projected {
                        match event {
                            ProjectionEvent::QuestComplete { quest_id, .. } => {
                                self.state.quests.completed(quest_id as f64)
                            }
                            ProjectionEvent::Logout => {
                                self.state.reset(&self.link);
                                self.state.dirty = true;
                            }
                            ProjectionEvent::Login => self.state.dirty = true,
                            _ => {}
                        }
                    }
                }
                Ok(Some(ScriptEvent::Invalidated)) | Err(Error::Overflow) => {
                    self.state.reset(&self.link);
                    self.state.dirty = true;
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => {
                    self.state.reset(&self.link);
                    return None;
                }
            }
            if index == 63 {
                return Some(now);
            }
        }
        self.state.advance(&self.link, &self.waker, now)
    }
}
struct Environment {
    config: EnvironmentState,
    dirty: bool,
    refresh: Option<Read<Snapshot>>,
    retry: Option<Instant>,
    quests: quests::Quests,
    boosts: boosts::Boosts,
    failures: Failures,
}
impl Environment {
    fn new() -> Self {
        Self {
            config: EnvironmentState::default(),
            dirty: true,
            refresh: None,
            retry: None,
            quests: Default::default(),
            boosts: Default::default(),
            failures: Default::default(),
        }
    }
    fn reset(&mut self, host: &impl Host) {
        self.quests.reset(host);
        self.boosts.reset();
        self.refresh = None;
        self.retry = None;
        self.config = Default::default();
        self.failures = Default::default();
    }
    fn advance(&mut self, host: &impl Host, waker: &Waker, now: Instant) -> Option<Instant> {
        if self.retry.is_some_and(|at| now < at) {
            return self.retry;
        }
        self.retry = None;
        if self.dirty && self.refresh.is_none() {
            self.dirty = false;
            self.refresh = Some(Read::new(host.snapshot(false), waker, now));
        }
        if let Some(read) = &self.refresh
            && let Some(result) = read.poll(now)
        {
            self.refresh = None;
            match result {
                Ok(snapshot) => {
                    self.config = snapshot.config;
                    self.quests.configure(
                        host,
                        self.config.automation.quests && !self.config.quest_ids.is_empty(),
                        now,
                    );
                    self.boosts.configure(
                        self.config.automation.boosts && !self.config.boosts.is_empty(),
                        now,
                    );
                    if !self.config.automation.quests {
                        self.failures.clear("quests");
                    }
                    if !self.config.automation.boosts {
                        self.failures.clear("boosts");
                    }
                    self.failures.set("snapshot", true);
                }
                Err(_) => {
                    self.failures.set("snapshot", false);
                    self.dirty = true;
                    self.retry = Some(now + QUEST_INTERVAL);
                }
            }
        }
        if let Some(read) = &self.refresh {
            return Some(read.deadline);
        }
        if self.retry.is_some() {
            return self.retry;
        }
        if self.dirty {
            return Some(now);
        }
        [
            self.refresh.as_ref().map(|r| r.deadline),
            self.retry,
            self.quests
                .advance(host, waker, &self.config, &mut self.failures, now),
            self.boosts
                .advance(host, waker, &self.config, &mut self.failures, now),
        ]
        .into_iter()
        .flatten()
        .min()
    }
}

#[cfg(test)]
mod tests;
