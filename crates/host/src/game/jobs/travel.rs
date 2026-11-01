use super::*;
use crate::bridge::Value;
use crate::game::travel::{self, Effects, Event, Outcome, Reads, Reply, Snapshot, TravelJob};

pub(crate) enum TravelOwner {
    Script(AttachmentId),
    Session(Option<crate::bridge::Reply<Outcome>>),
}
impl From<crate::bridge::Reply<Outcome>> for TravelOwner {
    fn from(reply: crate::bridge::Reply<Outcome>) -> Self {
        Self::Session(Some(reply))
    }
}
impl TravelOwner {
    fn script(&self) -> bool {
        matches!(self, Self::Script(_))
    }
    pub(super) fn complete(&mut self, outcome: Outcome, id: JobId, inbox: &Inbox) {
        match self {
            Self::Script(attachment) => {
                if let Outcome::Returned(value) = outcome {
                    inbox.post(Msg::OperationAnswer(
                        *attachment,
                        ScriptEvent::TravelDone { id: id.0, value },
                    ));
                }
            }
            Self::Session(reply) => {
                if let Some(reply) = reply.take() {
                    let _ = reply.send(Ok(outcome));
                }
            }
        }
    }
}

const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct Request {
    pub(super) id: JobId,
    pub(super) owner: TravelOwner,
    pub(super) detached: Arc<AtomicBool>,
    pub(super) player: Endpoint,
    pub(super) gate: Arc<JobGate>,
    pub(super) executor: TravelJob,
}

pub(super) struct Travel {
    request: Request,
    snapshot: Snapshot,
    due: Option<Instant>,
    waiting: Option<Waiting>,
    done: bool,
}
enum Waiting {
    Observation {
        pending: Pending<Snapshot>,
        deadline: Instant,
    },
    Action {
        pending: Pending<Value>,
        command: travel::Command,
        deadline: Instant,
    },
}

impl ScriptJobs {
    pub(crate) fn start_travel(&self, json: &str) -> Result<Option<JobId>, String> {
        let request =
            serde_json::from_str(json).map_err(|error| format!("invalid travel job: {error}"))?;
        let executor = match TravelJob::new(request) {
            Ok(executor) => executor,
            Err(_) => return Ok(None),
        };
        self.submit_travel(executor, TravelOwner::Script(self.attachment))
            .map(Some)
    }
    pub(crate) fn submit_travel(
        &self,
        executor: TravelJob,
        owner: TravelOwner,
    ) -> Result<JobId, String> {
        let script = owner.script();
        let mut handoff = self.requests.handoff.lock();
        if self.detached.load(Ordering::Acquire) || self.player.stopped.load(Ordering::Acquire) {
            return Err(Error::Stopped.to_string());
        }
        let waker = handoff
            .waker
            .clone()
            .ok_or("travel jobs require a session controller")?;
        self.live.lock().retain(|_, gate| gate.open());
        if script && handoff.scripts >= owner::MAX_JOBS {
            return Err("too many script jobs".into());
        }
        let id = JobId::next();
        let gate = Arc::new(JobGate::new(self.player.packets.binding()));
        self.live.lock().insert(id, gate.clone());
        handoff.travel.push_back(Request {
            id,
            owner,
            detached: self.detached.clone(),
            player: self.player.clone(),
            gate,
            executor,
        });
        handoff.travel_live += 1;
        handoff.scripts += usize::from(script);
        drop(handoff);
        waker.wake();
        Ok(id)
    }
}
impl ProfileJobs {
    pub(super) fn advance_travel(&mut self, now: Instant) -> Option<Instant> {
        let requests: Vec<_> = self.requests.handoff.lock().travel.drain(..).collect();
        for request in requests {
            self.travel.push(Travel {
                request,
                snapshot: Snapshot::default(),
                due: Some(now),
                waiting: None,
                done: false,
            });
        }
        for travel in &mut self.travel {
            travel.turn(now, self.clock, &self.link, &self.waker);
        }
        let before = self.travel.len();
        let finished_scripts = self
            .travel
            .iter()
            .filter(|job| job.done && job.request.owner.script())
            .count();
        self.travel.retain(|job| !job.done);
        let mut handoff = self.requests.handoff.lock();
        let finished = before - self.travel.len();
        handoff.travel_live -= finished;
        handoff.scripts -= finished_scripts;
        self.travel.iter().filter_map(Travel::deadline).min()
    }
}
impl Travel {
    pub(super) fn cancel(&mut self) {
        self.request.gate.close();
        self.request.owner.complete(
            Outcome::Cancelled,
            self.request.id,
            &self.request.player.inbox,
        );
        self.request
            .executor
            .advance(&self.snapshot, Event::Cancel, 0);
        self.waiting = None;
        self.due = None;
        self.done = true;
    }
    fn deadline(&self) -> Option<Instant> {
        let reply = self.waiting.as_ref().map(|waiting| match waiting {
            Waiting::Observation { deadline, .. } | Waiting::Action { deadline, .. } => *deadline,
        });
        self.due.into_iter().chain(reply).min()
    }
    fn apply(
        &mut self,
        effects: Effects,
        now: Instant,
        clock: JobClock,
        link: &ScriptLink,
        waker: &Waker,
    ) {
        self.due = effects.next_deadline.map(|ms| clock.at(ms));
        if let Some(outcome) = effects.outcome {
            self.done = true;
            self.request.gate.close();
            self.waiting = None;
            self.due = None;
            self.request
                .owner
                .complete(outcome, self.request.id, &link.inbox());
        } else if let Some(command) = effects.command {
            let deadline = now + REPLY_TIMEOUT;
            let (reply, pending) = Pending::channel();
            pending.set_waker(waker.clone());
            link.inbox().post(Msg::DispatchTravel {
                action: command.action.clone(),
                gate: self.request.gate.clone(),
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
    fn turn(&mut self, now: Instant, clock: JobClock, link: &ScriptLink, waker: &Waker) {
        if self.request.detached.load(Ordering::Acquire)
            || !self.request.gate.open()
            || !Arc::ptr_eq(&self.request.player.packets, &link.packets)
            || link.player_status().is_stopped()
        {
            self.cancel();
            return;
        }
        if self.due.is_some_and(|due| due <= now)
            && matches!(self.waiting, Some(Waiting::Action { .. }))
        {
            self.waiting = None;
            let effects = self
                .request
                .executor
                .advance(&self.snapshot, Event::Tick, clock.ms(now));
            self.apply(effects, now, clock, link, waker);
            return;
        }
        if let Some(waiting) = self.waiting.take() {
            let event = match waiting {
                Waiting::Observation { pending, deadline } => {
                    let result = pending
                        .try_recv()
                        .or_else(|| (now >= deadline).then_some(Err(Error::Timeout)));
                    let Some(result) = result else {
                        self.waiting = Some(Waiting::Observation { pending, deadline });
                        return;
                    };
                    self.snapshot = match result {
                        Ok(snapshot) => snapshot,
                        Err(_)
                            if self
                                .request
                                .executor
                                .observation_deadline()
                                .is_some_and(|at| clock.ms(now) >= at) =>
                        {
                            Snapshot::default()
                        }
                        Err(_) => {
                            self.apply(
                                Effects {
                                    outcome: Some(Outcome::Returned(false)),
                                    ..Default::default()
                                },
                                now,
                                clock,
                                link,
                                waker,
                            );
                            return;
                        }
                    };
                    Event::Tick
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
                        return;
                    };
                    let reply = if matches!(command.action, travel::Action::CellReady(_)) {
                        Reply::CellReady(result == Ok(Value::Bool(true)))
                    } else if result.is_ok() {
                        Reply::Accepted
                    } else {
                        Reply::Unavailable
                    };
                    Event::Completed {
                        id: command.id,
                        reply,
                    }
                }
            };
            let effects = self
                .request
                .executor
                .advance(&self.snapshot, event, clock.ms(now));
            self.apply(effects, now, clock, link, waker);
            return;
        }
        if !self.due.is_some_and(|due| due <= now) {
            return;
        }
        let reads = self.request.executor.reads();
        if reads == Reads::default() {
            let effects = self
                .request
                .executor
                .advance(&self.snapshot, Event::Tick, clock.ms(now));
            self.apply(effects, now, clock, link, waker);
        } else {
            let (reply, pending) = Pending::channel();
            pending.set_waker(waker.clone());
            let deadline = self
                .request
                .executor
                .observation_deadline()
                .map(|at| clock.at(at))
                .unwrap_or(now + REPLY_TIMEOUT)
                .min(now + REPLY_TIMEOUT);
            link.inbox().post(Msg::ObserveTravel {
                reads,
                gate: self.request.gate.clone(),
                deadline,
                reply,
            });
            self.due = None;
            self.waiting = Some(Waiting::Observation { pending, deadline });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn an_unanswered_travel_action_fails_at_its_reply_deadline() {
        let host = testing::headless_host();
        let (_session, link) = testing::start_probe(&host, testing::ProbeMovie::new());
        let now = Instant::now();
        let clock = JobClock { origin: now };
        let mut executor = TravelJob::new(travel::Request::Jump {
            cell: "r2".into(),
            pad: None,
        })
        .unwrap();
        let command = executor
            .advance(&Snapshot::default(), Event::Tick, 0)
            .command
            .unwrap();
        let gate = Arc::new(JobGate::new(link.packets.binding()));
        let (_reply, pending) = Pending::channel();
        let mut job = Travel {
            request: Request {
                id: JobId::next(),
                owner: TravelOwner::Script(link.jobs.attachment),
                detached: Arc::default(),
                player: link.jobs.player.clone(),
                gate,
                executor,
            },
            snapshot: Snapshot::default(),
            due: None,
            waiting: Some(Waiting::Action {
                pending,
                command,
                deadline: now + REPLY_TIMEOUT,
            }),
            done: false,
        };
        job.turn(
            now + Duration::from_millis(4999),
            clock,
            &link,
            Waker::noop(),
        );
        assert!(!job.done);
        job.turn(now + Duration::from_secs(5), clock, &link, Waker::noop());
        assert!(job.done);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(Some(ScriptEvent::TravelDone { value, .. })) = link.try_event() {
                assert!(!value);
                break;
            }
            assert!(Instant::now() < deadline, "travel did not finish");
            std::thread::yield_now();
        }
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use crate::testing;

    #[test]
    fn revoking_a_run_closes_queued_travel_before_issuance() {
        let host = testing::headless_host();
        let (_session, link) = testing::start_probe(&host, testing::ProbeMovie::new());
        let jobs = ProfileJobs::new(link, Waker::noop().clone());
        let id = jobs
            .link
            .jobs
            .start_travel(r#"{"kind":"jump","cell":"r2"}"#)
            .unwrap()
            .unwrap();
        let gate = jobs
            .requests
            .handoff
            .lock()
            .travel
            .front()
            .unwrap()
            .gate
            .clone();
        assert!(gate.open());
        jobs.link.jobs.revoke();
        assert!(!gate.open());
        assert_eq!(jobs.requests.handoff.lock().travel.front().unwrap().id, id);
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use crate::testing;

    #[test]
    fn session_travel_returns_its_outcome_on_the_controller() {
        let host = testing::headless_host();
        let (_session, link) = testing::start_probe(&host, testing::ProbeMovie::new());
        let mut jobs = ProfileJobs::new(link, Waker::noop().clone());
        let (reply, pending) = Pending::channel();
        jobs.link
            .jobs
            .submit_travel(TravelJob::new(travel::Request::Exit).unwrap(), reply.into())
            .unwrap();
        assert_eq!(jobs.requests.handoff.lock().scripts, 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            jobs.advance(Instant::now());
            if let Some(result) = pending.try_recv() {
                assert_eq!(result.unwrap(), Outcome::Returned(true));
                break;
            }
            assert!(Instant::now() < deadline, "session travel did not finish");
            std::thread::yield_now();
        }
        assert_eq!(jobs.requests.handoff.lock().travel_live, 0);
    }
}

#[cfg(test)]
mod observation_deadline_tests {
    use super::*;
    use crate::testing;
    use std::sync::mpsc;

    #[test]
    fn a_live_location_read_cannot_outlive_the_jump() {
        let host = testing::headless_host();
        let (_session, link) = testing::start_probe(&host, testing::ProbeMovie::new());
        let now = Instant::now();
        let clock = JobClock { origin: now };
        let mut executor = TravelJob::new(travel::Request::Jump {
            cell: "r2".into(),
            pad: None,
        })
        .unwrap();
        let command = executor
            .advance(&Snapshot::default(), Event::Tick, 0)
            .command
            .unwrap();
        executor.advance(
            &Snapshot::default(),
            Event::Completed {
                id: command.id,
                reply: Reply::Accepted,
            },
            0,
        );
        let (entered, arrived) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let _hold = link.inbox().read_projection(move |_| {
            let _ = entered.send(());
            let _ = held.recv();
        });
        arrived.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut job = Travel {
            request: Request {
                id: JobId::next(),
                owner: TravelOwner::Script(link.jobs.attachment),
                detached: Arc::default(),
                player: link.jobs.player.clone(),
                gate: Arc::new(JobGate::new(link.packets.binding())),
                executor,
            },
            snapshot: Snapshot::default(),
            due: Some(now),
            waiting: None,
            done: false,
        };
        job.turn(now, clock, &link, Waker::noop());
        job.turn(now + Duration::from_secs(3), clock, &link, Waker::noop());
        let _ = release.send(());
        assert!(job.done, "a location read exceeded the jump deadline");
    }
}
