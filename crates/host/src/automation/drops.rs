use std::{
    collections::VecDeque,
    task::Waker,
    time::{Duration, Instant},
};

use lucent_protocol::{
    command::Command,
    environment::{EnvironmentItemBucket, EnvironmentItemRules},
    event::ProjectionEvent,
    item::ItemContainer,
    operation::Operation,
    query::ItemSelector,
};

use crate::{
    ScriptLink,
    bridge::{Error, EventInterests, EventKind, Pending, ScriptEvent},
    projection::wire::trim_js,
};

const READ_RETRY: Duration = Duration::from_millis(10);
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, PartialEq)]
struct Config {
    enabled: bool,
    names: Vec<String>,
    rules: EnvironmentItemRules,
}
impl Config {
    fn active(&self) -> bool {
        self.enabled
            && (!self.names.is_empty() || !self.rules.buckets.is_empty() || self.rules.reject_else)
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Action {
    Accept,
    Reject,
    Ignore,
}

struct Attempt {
    item: i64,
    retries: u32,
    at: Instant,
}
struct Pass {
    waiting: VecDeque<Attempt>,
    failed: Vec<i64>,
    flight: Option<Flight>,
}
struct Flight {
    attempt: Attempt,
    state: Mutation,
    deadline: Instant,
}
enum Mutation {
    Accept(u64),
    Login(Pending<String>),
    Reject(Pending<String>),
}

pub struct DropPolicy {
    link: ScriptLink,
    waker: Waker,
    config: Option<Config>,
    rescan: bool,
    pass: Option<Pass>,
}
impl DropPolicy {
    pub const INTERESTS: EventInterests = EventInterests::new(&[
        EventKind::ItemDrop,
        EventKind::Invalidated,
        EventKind::Operation,
    ]);

    pub fn new(link: ScriptLink, waker: Waker) -> Self {
        link.set_waker(Waker::noop().clone());
        Self {
            link,
            waker,
            config: None,
            rescan: false,
            pass: None,
        }
    }

    pub fn advance(&mut self, now: Instant) -> Option<Instant> {
        if self.link.player_status().is_stopped() {
            return None;
        }
        let Some(state) = self.link.records.environment.try_read() else {
            return Some(now + READ_RETRY);
        };
        let config = Config {
            enabled: state.automation.drops,
            names: state.item_names.clone(),
            rules: state.item_rules.clone(),
        };
        drop(state);
        if self.config.as_ref() != Some(&config) {
            self.rescan |= config.active();
            self.link.set_waker(if config.active() {
                self.waker.clone()
            } else {
                Waker::noop().clone()
            });
            self.config = Some(config);
        }
        if !self.config.as_ref().is_some_and(Config::active) {
            self.rescan = false;
            if self.pass.is_some() {
                self.cancel_pass();
                tracing::debug!("environment drops reconciled");
            }
            for _ in 0..64 {
                if matches!(self.link.try_event(), Ok(None) | Err(Error::Stopped)) {
                    break;
                }
            }
            return None;
        }
        let mut drained = false;
        for index in 0..64 {
            match self.link.try_event() {
                Ok(Some(ScriptEvent::Invalidated)) | Err(Error::Overflow) => {
                    self.cancel_pass();
                    self.rescan = self.config.as_ref().is_some_and(Config::active);
                }
                Ok(Some(ScriptEvent::Loader { projected, .. })) => {
                    if projected
                        .iter()
                        .any(|e| matches!(e, ProjectionEvent::ItemDrop { .. }))
                    {
                        self.rescan |= self.config.as_ref().is_some_and(Config::active);
                    }
                }
                Ok(Some(ScriptEvent::Operation { id, .. })) => {
                    if let Some(pass) = &mut self.pass
                        && pass.flight.as_ref().is_some_and(|flight| matches!(flight.state, Mutation::Accept(current) if current == id)) {
                        pass.flight = None;
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    drained = true;
                    break;
                }
                Err(_) => {
                    self.cancel_pass();
                    return None;
                }
            }
            if index == 63 {
                self.waker.wake_by_ref();
            }
        }
        if !drained {
            return Some(now);
        }
        for _ in 0..64 {
            if self.pass.is_none() {
                if !self.rescan {
                    return None;
                }
                let items = match self.link.read_for(Duration::ZERO, |p| {
                    p.items
                        .all(ItemContainer::Drop)
                        .iter()
                        .map(|item| item.data().item_id)
                        .collect::<Vec<_>>()
                }) {
                    Ok(items) => items,
                    Err(Error::Timeout) => return Some(now + READ_RETRY),
                    Err(_) => return None,
                };
                self.rescan = false;
                self.pass = Some(Pass {
                    waiting: items
                        .into_iter()
                        .map(|item| Attempt {
                            item,
                            retries: 0,
                            at: now,
                        })
                        .collect(),
                    failed: vec![],
                    flight: None,
                });
            }
            if self.pass.as_ref().unwrap().flight.is_some() {
                return self.poll_flight(now);
            }
            let pass = self.pass.as_mut().unwrap();
            if pass.waiting.is_empty() {
                if !pass.failed.is_empty() {
                    tracing::warn!(
                        "environment drop rejection retries exhausted: {:?}",
                        pass.failed
                    );
                }
                tracing::debug!("environment drops reconciled");
                self.pass = None;
                continue;
            }
            let Some(index) = pass.waiting.iter().position(|attempt| attempt.at <= now) else {
                return pass.waiting.iter().map(|attempt| attempt.at).min();
            };
            let attempt = pass.waiting.remove(index).unwrap();
            let action = match self.action(attempt.item) {
                Ok(action) => action,
                Err(Error::Timeout) => {
                    self.pass.as_mut().unwrap().waiting.push_front(attempt);
                    return Some(now + READ_RETRY);
                }
                Err(_) => {
                    self.cancel_pass();
                    return None;
                }
            };
            if action == Action::Ignore || (attempt.retries > 0 && action != Action::Reject) {
                continue;
            }
            let state = match action {
                Action::Accept => match self.link.operate(Operation::AcceptDrop {
                    selector: ItemSelector::Id(attempt.item as f64),
                }) {
                    Ok(id) => Mutation::Accept(id),
                    Err(_) => continue,
                },
                Action::Reject => {
                    let pending = self.link.command(Command::RefreshLoggedIn {});
                    pending.set_waker(self.waker.clone());
                    Mutation::Login(pending)
                }
                Action::Ignore => unreachable!(),
            };
            self.pass.as_mut().unwrap().flight = Some(Flight {
                attempt,
                state,
                deadline: now + REPLY_TIMEOUT,
            });
            return Some(now + REPLY_TIMEOUT);
        }
        Some(now)
    }

    fn action(&self, id: i64) -> Result<Action, Error> {
        let state = self
            .link
            .records
            .environment
            .try_read()
            .ok_or(Error::Timeout)?;
        if !state.automation.drops {
            return Ok(Action::Ignore);
        }
        self.link.read_for(Duration::ZERO, |p| {
            let Some(item) = p.items.get(ItemContainer::Drop, id) else {
                return Action::Ignore;
            };
            let item = item.data();
            let bucket = match (item.coins, item.member_only) {
                (true, true) => EnvironmentItemBucket::AcMember,
                (true, false) => EnvironmentItemBucket::AcNonMember,
                (false, true) => EnvironmentItemBucket::NonAcMember,
                (false, false) => EnvironmentItemBucket::NonAcNonMember,
            };
            let name = trim_js(&item.name).to_lowercase();
            if state
                .item_names
                .iter()
                .any(|registered| registered.to_lowercase() == name)
                || state.item_rules.buckets.contains(&bucket)
            {
                Action::Accept
            } else if state.item_rules.reject_else {
                Action::Reject
            } else {
                Action::Ignore
            }
        })
    }

    fn poll_flight(&mut self, now: Instant) -> Option<Instant> {
        let flight = self.pass.as_mut().unwrap().flight.take().unwrap();
        let result = match &flight.state {
            Mutation::Accept(_) => None,
            Mutation::Login(pending) | Mutation::Reject(pending) => pending
                .try_recv()
                .map(|r| matches!(r, Ok(value) if value == "true"))
                .or_else(|| (now >= flight.deadline).then_some(false)),
        };
        let Some(success) = result else {
            let next = (!matches!(flight.state, Mutation::Accept(_))).then_some(flight.deadline);
            self.pass.as_mut().unwrap().flight = Some(flight);
            return next;
        };
        if matches!(flight.state, Mutation::Login(_)) && success {
            match self.action(flight.attempt.item) {
                Ok(Action::Reject) => {
                    let pending = self.link.command(Command::RejectDrop {
                        selector: ItemSelector::Id(flight.attempt.item as f64),
                    });
                    pending.set_waker(self.waker.clone());
                    self.pass.as_mut().unwrap().flight = Some(Flight {
                        state: Mutation::Reject(pending),
                        deadline: now + REPLY_TIMEOUT,
                        ..flight
                    });
                    return Some(now + REPLY_TIMEOUT);
                }
                Err(Error::Timeout) => {
                    self.pass.as_mut().unwrap().waiting.push_back(Attempt {
                        at: now + READ_RETRY,
                        ..flight.attempt
                    });
                    return Some(now + READ_RETRY);
                }
                _ => return Some(now),
            }
        }
        if !success && !matches!(flight.state, Mutation::Accept(_)) {
            let mut attempt = flight.attempt;
            if attempt.retries < 3 {
                attempt.at = now + Duration::from_millis(100 << attempt.retries);
                attempt.retries += 1;
                self.pass.as_mut().unwrap().waiting.push_back(attempt);
            } else {
                self.pass.as_mut().unwrap().failed.push(attempt.item);
            }
        }
        Some(now)
    }

    fn cancel_pass(&mut self) {
        if let Some(pass) = self.pass.take()
            && let Some(Flight {
                state: Mutation::Accept(id),
                ..
            }) = pass.flight
        {
            self.link.cancel_operation(id);
        }
    }
}
impl Drop for DropPolicy {
    fn drop(&mut self) {
        self.cancel_pass();
    }
}
