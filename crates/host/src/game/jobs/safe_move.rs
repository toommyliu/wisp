use super::*;
use crate::game::{safe_move as sm, travel};

const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct Guard {
    pub(crate) gate: Arc<JobGate>,
    username: String,
}
impl Guard {
    pub(crate) fn new(link: &ScriptLink, username: &str) -> Arc<Self> {
        Arc::new(Self {
            gate: Arc::new(JobGate::new(link.packets.binding())),
            username: username.trim().to_lowercase(),
        })
    }
    pub(crate) fn close(&self) {
        self.gate.close();
    }
    pub(crate) fn allows(&self, binding: Binding, username: &str) -> bool {
        self.gate.open()
            && self.gate.state.lock().binding == binding
            && self.username == username.trim().to_lowercase()
    }
}

pub(crate) struct SafeMoveJob {
    link: ScriptLink,
    executor: sm::SafeMove,
    snapshot: sm::Snapshot,
    guard: Arc<Guard>,
    cleanup: Arc<Guard>,
    clock: JobClock,
    due: Option<Instant>,
    waiting: Option<Waiting>,
    cancelling: bool,
    outcome: Option<sm::Outcome>,
}
enum Waiting {
    Observation {
        pending: Pending<sm::Snapshot>,
        deadline: Instant,
    },
    Action {
        pending: Pending<Value>,
        command: sm::Command,
        deadline: Instant,
    },
}
use crate::bridge::Value;
impl SafeMoveJob {
    pub(crate) fn new(
        link: &ScriptLink,
        policy: sm::RoomPolicy,
        username: &str,
        now: Instant,
    ) -> Self {
        Self {
            link: link.fork(EventInterests::new(&[])),
            executor: sm::SafeMove::new(policy).expect("validated room policy"),
            snapshot: sm::Snapshot {
                logged_in: false,
                username: String::new(),
                equipped_house: false,
                own_house: false,
                travel: Default::default(),
            },
            guard: Guard::new(link, username),
            cleanup: Guard::new(link, username),
            clock: JobClock { origin: now },
            due: Some(now),
            waiting: None,
            cancelling: false,
            outcome: None,
        }
    }
    pub(crate) fn cancel(&mut self) {
        self.guard.gate.close();
        self.cancelling = true;
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.due
            .into_iter()
            .chain(self.waiting.as_ref().map(|waiting| match waiting {
                Waiting::Observation { deadline, .. } | Waiting::Action { deadline, .. } => {
                    *deadline
                }
            }))
            .min()
    }
    fn apply(&mut self, effects: sm::Effects, now: Instant, waker: &Waker) {
        self.due = effects.next_deadline.map(|at| self.clock.at(at));
        if let Some(outcome) = effects.outcome {
            self.outcome = Some(outcome);
            self.waiting = None;
            self.due = None;
            self.guard.gate.close();
            self.cleanup.gate.close();
        } else if let Some(command) = effects.command {
            let deadline = self
                .due
                .unwrap_or(now + REPLY_TIMEOUT)
                .min(now + REPLY_TIMEOUT);
            self.due = None;
            let (reply, pending) = Pending::channel();
            pending.set_waker(waker.clone());
            self.link.inbox().post(Msg::DispatchSafeMove {
                action: command.action.clone(),
                guard: if self.executor.restoring() {
                    self.cleanup.clone()
                } else {
                    self.guard.clone()
                },
                deadline,
                reply,
            });
            self.waiting = Some(Waiting::Action {
                pending,
                command,
                deadline,
            });
        }
    }
    pub(crate) fn advance(&mut self, now: Instant, waker: &Waker) -> Option<sm::Outcome> {
        if self.outcome.is_some() {
            return self.outcome.take();
        }
        if self.link.player_status().is_stopped()
            || self.link.packets.binding() != self.guard.gate.state.lock().binding
        {
            self.cancel();
        }
        let event = if let Some(waiting) = self.waiting.take() {
            match waiting {
                Waiting::Observation { pending, deadline } => {
                    let result = pending
                        .try_recv()
                        .or_else(|| (now >= deadline).then_some(Err(Error::Timeout)));
                    let Some(result) = result else {
                        self.waiting = Some(Waiting::Observation { pending, deadline });
                        return None;
                    };
                    match result {
                        Ok(snapshot) => self.snapshot = snapshot,
                        Err(error) => {
                            self.guard.gate.close();
                            let effects = self.executor.fail(error.to_string());
                            self.apply(effects, now, waker);
                            return self.outcome.take();
                        }
                    }
                    sm::Event::Tick
                }
                Waiting::Action {
                    pending,
                    command,
                    deadline,
                } => {
                    let result = pending
                        .try_recv()
                        .or_else(|| (now >= deadline).then_some(Err(Error::Timeout)));
                    let Some(result) = result else {
                        self.waiting = Some(Waiting::Action {
                            pending,
                            command,
                            deadline,
                        });
                        return None;
                    };
                    sm::Event::Completed {
                        id: command.id,
                        reply: decode(&command.action, result),
                    }
                }
            }
        } else if self.cancelling {
            sm::Event::Cancel
        } else if self.due.is_some_and(|at| now >= at) {
            let (reply, pending) = Pending::channel();
            pending.set_waker(waker.clone());
            let deadline = self
                .executor
                .observation_deadline()
                .map(|at| self.clock.at(at))
                .unwrap_or(now + REPLY_TIMEOUT)
                .min(now + REPLY_TIMEOUT);
            self.link.inbox().post(Msg::ObserveSafeMove {
                reads: self.executor.reads(),
                own_house: self.executor.reads_house(),
                guard: self.guard.clone(),
                deadline,
                reply,
            });
            self.due = None;
            self.waiting = Some(Waiting::Observation { pending, deadline });
            return None;
        } else {
            return None;
        };
        let mut effects = self
            .executor
            .advance(&self.snapshot, event, self.clock.ms(now));
        if self.cancelling && effects.outcome.is_none() && !self.executor.restoring() {
            effects = self
                .executor
                .advance(&self.snapshot, sm::Event::Cancel, self.clock.ms(now));
        }
        self.apply(effects, now, waker);
        self.outcome.take()
    }
}
impl Drop for SafeMoveJob {
    fn drop(&mut self) {
        self.guard.gate.close();
        self.cleanup.gate.close();
    }
}
fn decode(action: &sm::Action, result: Result<Value, Error>) -> sm::Reply {
    match (action, result) {
        (sm::Action::Travel(action), result) => {
            sm::Reply::Travel(if matches!(action, travel::Action::CellReady(_)) {
                travel::Reply::CellReady(result == Ok(Value::Bool(true)))
            } else if result.is_ok() {
                travel::Reply::Accepted
            } else {
                travel::Reply::Unavailable
            })
        }
        (_, Err(Error::Stopped | Error::NotReady | Error::Unavailable(_))) => {
            sm::Reply::Unavailable
        }
        (_, Err(error)) => sm::Reply::Error(error.to_string()),
        (sm::Action::GetReturnInfo, Ok(Value::String(json))) => serde_json::from_str(&json)
            .map(sm::Reply::Value)
            .unwrap_or(sm::Reply::Unavailable),
        (sm::Action::GetReturnInfo, Ok(_)) => sm::Reply::Unavailable,
        (sm::Action::IsReturnInfoNull, Ok(value)) => lucent_protocol::bridge::to_json(value)
            .map(sm::Reply::Value)
            .unwrap_or(sm::Reply::Unavailable),
        (sm::Action::SetReturnInfo(_) | sm::Action::House(_), Ok(_)) => sm::Reply::Accepted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    #[test]
    fn travel_failures_keep_the_travel_reply_contract() {
        for error in [Error::Timeout, Error::Unavailable("player.joinMap".into())] {
            assert!(matches!(
                decode(
                    &sm::Action::Travel(travel::Action::Join(vec!["buyhouse".into()])),
                    Err(error)
                ),
                sm::Reply::Travel(travel::Reply::Unavailable)
            ));
        }
    }
    #[test]
    fn an_unanswered_observation_closes_its_gate_at_five_seconds() {
        let host = testing::headless_host();
        let (_session, link) = testing::start_probe(
            &host,
            testing::ProbeMovie::new().recording("world.isLoaded"),
        );
        let (entered, arrived) = std::sync::mpsc::channel();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let _hold = link.inbox().read_projection(move |_| {
            let _ = entered.send(());
            let _ = held.recv();
        });
        arrived.recv_timeout(Duration::from_secs(5)).unwrap();
        let now = Instant::now();
        let mut job = SafeMoveJob::new(&link, sm::RoomPolicy::Public, "", now);
        assert!(job.advance(now, Waker::noop()).is_none());
        let outcome = job.advance(now + REPLY_TIMEOUT, Waker::noop());
        drop(release);
        assert!(matches!(outcome, Some(sm::Outcome::Error(_))));
        assert!(!job.guard.gate.open());
        link.inbox()
            .read_projection(|_| ())
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        while let Some(event) = link.try_event().unwrap() {
            if let ScriptEvent::Loader { event, .. } = event {
                assert_ne!(event.name, "onProbeCall");
            }
        }
    }
}
