use super::travel::{self, TravelJob};
use serde_json::Value;

#[cfg(test)]
mod tests;

mod types;
pub use types::*;

fn bounded_return_info(value: &Value) -> bool {
    let mut pending = vec![(value, 0)];
    let mut nodes = 0;
    let mut bytes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        nodes += 1;
        if nodes > 4096 || depth > 128 {
            return false;
        }
        match value {
            Value::String(text) => bytes = bytes.saturating_add(text.len()),
            Value::Array(items) => {
                if nodes + pending.len() + items.len() > 4096 {
                    return false;
                }
                pending.extend(items.iter().map(|item| (item, depth + 1)));
            }
            Value::Object(items) => {
                if nodes + pending.len() + items.len() > 4096 {
                    return false;
                }
                for (key, item) in items {
                    bytes = bytes.saturating_add(key.len());
                    pending.push((item, depth + 1));
                }
            }
            _ => {}
        }
        if bytes > 65536 {
            return false;
        }
    }
    value.to_string().len() <= 65536
}

#[derive(Clone, Debug)]
enum Destination {
    House(String),
    Buyhouse(String),
}
#[derive(Debug)]
enum Pending {
    IsNull,
    Read,
    Clear(Value),
    House,
    Restore(Outcome),
}
#[derive(Debug)]
enum TravelKind {
    Exit,
    Join,
}
#[derive(Debug)]
enum Phase {
    New,
    Respawn(u64),
    Travel {
        job: Box<TravelJob>,
        kind: TravelKind,
        pending: Option<(u64, u64)>,
    },
    Settle(u64),
    Pending {
        id: u64,
        kind: Pending,
    },
    Attempt,
    Ready(u64),
    Backoff(u64),
    Done,
}
#[derive(Debug)]
pub struct SafeMove {
    policy: RoomPolicy,
    phase: Phase,
    destination: Option<Destination>,
    saved_return_info: Option<Value>,
    attempts: u8,
    sequence: u64,
}
impl SafeMove {
    pub fn new(policy: RoomPolicy) -> Result<Self, InputError> {
        let valid = match policy {
            RoomPolicy::Public => true,
            RoomPolicy::Specific { room } => (1..=99999).contains(&room),
            RoomPolicy::RandomPrivate { room } => (10000..=99999).contains(&room),
        };
        if !valid {
            return Err(InputError::InvalidRoom);
        }
        Ok(Self {
            policy,
            phase: Phase::New,
            destination: None,
            saved_return_info: None,
            attempts: 0,
            sequence: 0,
        })
    }
    fn issue(&mut self, action: Action, kind: Pending) -> Effects {
        self.sequence += 1;
        self.phase = Phase::Pending {
            id: self.sequence,
            kind,
        };
        Effects {
            command: Some(Command {
                id: self.sequence,
                action,
            }),
            ..Default::default()
        }
    }
    fn finish(&mut self, outcome: Outcome) -> Effects {
        if let Some(value) = self.saved_return_info.take() {
            return self.issue(Action::SetReturnInfo(value), Pending::Restore(outcome));
        }
        self.phase = Phase::Done;
        Effects {
            outcome: Some(outcome),
            ..Default::default()
        }
    }
    fn travel(&mut self, request: travel::Request, kind: TravelKind) {
        self.phase = Phase::Travel {
            job: Box::new(TravelJob::new(request).expect("bounded safe destination")),
            kind,
            pending: None,
        };
    }
    fn retry(&mut self, now: u64) -> Effects {
        if self.attempts >= 3 {
            return self.finish(Outcome::TimedOut);
        }
        self.phase = Phase::Backoff(now.saturating_add(1000 << (self.attempts - 1)));
        self.tick(now)
    }
    fn tick(&self, at: u64) -> Effects {
        Effects {
            next_deadline: Some(at),
            ..Default::default()
        }
    }
    pub fn advance(&mut self, snapshot: &Snapshot, mut event: Event, now: u64) -> Effects {
        if matches!(self.phase, Phase::Done) {
            return Effects::default();
        }
        for _ in 0..8 {
            let phase = std::mem::replace(&mut self.phase, Phase::Done);
            match phase {
                Phase::New => {
                    if snapshot.username.len() > 4096 {
                        return self.finish(Outcome::Error("username exceeds limit".into()));
                    }
                    if !snapshot.logged_in || snapshot.username.trim().is_empty() {
                        return self.finish(Outcome::Skipped);
                    }
                    self.destination = Some(if snapshot.equipped_house {
                        if snapshot.own_house {
                            return self.finish(Outcome::Skipped);
                        }
                        Destination::House(snapshot.username.trim().to_owned())
                    } else {
                        let room = snapshot.travel.room;
                        let acceptable = match self.policy {
                            RoomPolicy::Public => true,
                            RoomPolicy::Specific { room: target } => room == u64::from(target),
                            RoomPolicy::RandomPrivate { .. } => (1001..=99999).contains(&room),
                        };
                        if snapshot.travel.map.eq_ignore_ascii_case("buyhouse") && acceptable {
                            return self.finish(Outcome::Skipped);
                        }
                        Destination::Buyhouse(match self.policy {
                            RoomPolicy::Public => "buyhouse".into(),
                            RoomPolicy::Specific { room } | RoomPolicy::RandomPrivate { room } => {
                                format!("buyhouse-{room}")
                            }
                        })
                    });
                    self.phase = Phase::Respawn(now.saturating_add(15000));
                }
                Phase::Respawn(expires) => {
                    if !snapshot.logged_in {
                        return self.finish(Outcome::Aborted);
                    }
                    if now >= expires {
                        return self.finish(Outcome::TimedOut);
                    }
                    if !snapshot.travel.alive {
                        self.phase = Phase::Respawn(expires);
                        return self.tick(now.saturating_add(100).min(expires));
                    }
                    self.travel(travel::Request::Exit, TravelKind::Exit);
                }
                Phase::Travel {
                    mut job,
                    kind,
                    mut pending,
                } => {
                    if !snapshot.logged_in {
                        return self.finish(Outcome::Aborted);
                    }
                    let travel_event = match std::mem::replace(&mut event, Event::Tick) {
                        Event::Completed { id, reply }
                            if pending.is_some_and(|(outer, _)| outer == id) =>
                        {
                            let (_, inner) = pending.take().unwrap();
                            match reply {
                                Reply::Travel(reply) => {
                                    travel::Event::Completed { id: inner, reply }
                                }
                                Reply::Error(message) => {
                                    return self.finish(Outcome::Error(message));
                                }
                                _ => travel::Event::Completed {
                                    id: inner,
                                    reply: travel::Reply::Unavailable,
                                },
                            }
                        }
                        _ => travel::Event::Tick,
                    };
                    let effects = job.advance(&snapshot.travel, travel_event, now);
                    if let Some(outcome) = effects.outcome {
                        match (kind, outcome) {
                            (TravelKind::Exit, travel::Outcome::Returned(true)) => {
                                self.phase = Phase::Settle(now.saturating_add(1000))
                            }
                            (TravelKind::Join, travel::Outcome::Returned(true)) => {
                                self.phase = Phase::Ready(now.saturating_add(5000))
                            }
                            (TravelKind::Join, _) => return self.retry(now),
                            (TravelKind::Exit, _) => return self.finish(Outcome::Aborted),
                        }
                        continue;
                    }
                    let command = effects.command.map(|command| {
                        self.sequence += 1;
                        pending = Some((self.sequence, command.id));
                        Command {
                            id: self.sequence,
                            action: Action::Travel(command.action),
                        }
                    });
                    self.phase = Phase::Travel { job, kind, pending };
                    return Effects {
                        command,
                        next_deadline: effects.next_deadline,
                        outcome: None,
                    };
                }
                Phase::Settle(at) => {
                    if now < at {
                        self.phase = Phase::Settle(at);
                        return self.tick(at);
                    }
                    if !snapshot.logged_in {
                        return self.finish(Outcome::Aborted);
                    }
                    return self.issue(Action::IsReturnInfoNull, Pending::IsNull);
                }
                Phase::Backoff(at) => {
                    if now < at {
                        self.phase = Phase::Backoff(at);
                        return self.tick(at);
                    }
                    self.phase = Phase::Attempt;
                }
                Phase::Pending { id, kind } => {
                    let reply = match std::mem::replace(&mut event, Event::Tick) {
                        Event::Completed { id: actual, reply } if id == actual => reply,
                        _ => {
                            self.phase = Phase::Pending { id, kind };
                            return Effects::default();
                        }
                    };
                    if let Pending::Restore(outcome) = kind {
                        return self.finish(match reply {
                            Reply::Error(message) => Outcome::Error(message),
                            _ => outcome,
                        });
                    }
                    if let Reply::Error(message) = reply {
                        return self.finish(Outcome::Error(message));
                    }
                    match kind {
                        Pending::IsNull => {
                            if matches!(reply, Reply::Value(Value::Bool(true))) {
                                self.phase = Phase::Attempt;
                            } else {
                                return self.issue(Action::GetReturnInfo, Pending::Read);
                            }
                        }
                        Pending::Read => match reply {
                            Reply::Value(value) if bounded_return_info(&value) => {
                                return self.issue(
                                    Action::SetReturnInfo(Value::Null),
                                    Pending::Clear(value),
                                );
                            }
                            _ => return self.finish(Outcome::Unavailable),
                        },
                        Pending::Clear(value) => {
                            if !matches!(reply, Reply::Accepted) {
                                return self.finish(Outcome::Unavailable);
                            }
                            self.saved_return_info = Some(value);
                            self.phase = Phase::Attempt;
                        }
                        Pending::House => {
                            if !matches!(reply, Reply::Accepted) {
                                return self.retry(now);
                            }
                            self.phase = Phase::Ready(now.saturating_add(5000));
                        }
                        Pending::Restore(_) => unreachable!(),
                    }
                }
                Phase::Attempt => {
                    if !snapshot.logged_in {
                        return self.finish(Outcome::Aborted);
                    }
                    self.attempts += 1;
                    match self.destination.as_ref().unwrap() {
                        Destination::House(username) => {
                            return self.issue(Action::House(username.clone()), Pending::House);
                        }
                        Destination::Buyhouse(target) => self.travel(
                            travel::Request::Join {
                                target: target.clone(),
                                cell: None,
                                pad: None,
                                random_room: None,
                            },
                            TravelKind::Join,
                        ),
                    }
                }
                Phase::Ready(expires) => {
                    if !snapshot.logged_in {
                        return self.finish(Outcome::Aborted);
                    }
                    if now >= expires {
                        return self.retry(now);
                    }
                    if snapshot.travel.ready
                        && (matches!(self.destination, Some(Destination::Buyhouse(_)))
                            || snapshot.own_house)
                    {
                        return self.finish(Outcome::Moved);
                    }
                    self.phase = Phase::Ready(expires);
                    return self.tick(now.saturating_add(100).min(expires));
                }
                Phase::Done => return Effects::default(),
            }
        }
        self.tick(now)
    }
}
