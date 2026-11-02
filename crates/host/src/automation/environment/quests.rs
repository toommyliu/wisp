use super::*;
use std::collections::{HashMap, VecDeque};

const SPACING: Duration = Duration::from_millis(750);

#[derive(Default)]
pub(super) struct Quests {
    next: Option<Instant>,
    cycle: Option<Cycle>,
    failures: HashMap<(i64, bool), Backoff>,
    unavailable: HashSet<i64>,
    last_mutation: Option<Instant>,
    settling: Vec<Mutation>,
}
struct Backoff {
    attempts: u32,
    after: Instant,
}
enum Cycle {
    Snapshot(Read<Snapshot>),
    Targets(Read<Snapshot>),
    Load {
        id: u64,
        deadline: Instant,
        answer: Option<bool>,
    },
    Intents {
        queue: VecDeque<f64>,
        probes: Vec<Probe>,
        intents: Vec<Intent>,
    },
    Mutations {
        queue: VecDeque<Intent>,
        active: Option<Mutation>,
    },
}
struct Probe {
    id: f64,
    progress: Option<bool>,
    read: Read<Value>,
}
#[derive(Clone, Copy)]
struct Intent {
    id: f64,
    complete: bool,
}
enum Outcome {
    Success,
    Failed,
    Unavailable,
    Settled,
}
struct Mutation {
    intent: Intent,
    _lease: Lease,
    stage: Stage,
    read: Option<Read<Value>>,
    deadline: Instant,
    poll: Instant,
    completed: bool,
    prepare: Option<Preparation>,
    action_ready: bool,
    issued: bool,
}
enum Preparation {
    Snapshot(Read<Snapshot>),
    Action(Read<Value>),
    Load { id: u64, answer: Option<bool> },
}
#[derive(Clone, Copy)]
enum Stage {
    Available,
    Progress,
    Completable,
    Action,
    Maximum,
    Dispatch(f64),
    Verify,
    Event,
}
impl Quests {
    pub fn configure(&mut self, host: &impl Host, enabled: bool, now: Instant) {
        if !enabled {
            self.reset(host);
        } else if self.next.is_none() && self.cycle.is_none() {
            self.next = Some(now + QUEST_INTERVAL);
        }
    }
    pub fn reset(&mut self, host: &impl Host) {
        if let Some(Cycle::Load { id, .. }) = self.cycle {
            host.cancel(id);
        }
        if let Some(Cycle::Mutations {
            active: Some(active),
            ..
        }) = &self.cycle
        {
            active.cancel(host);
        }
        let mut settling = std::mem::take(&mut self.settling);
        if let Some(Cycle::Mutations {
            active: Some(active),
            ..
        }) = self.cycle.take()
            && active.intent.complete
            && active.issued
            && !active.completed
        {
            settling.push(active);
        }
        *self = Self {
            settling,
            ..Default::default()
        };
    }
    pub fn loaded(&mut self, answered: u64, result: Result<OperationAnswer, Error>) {
        let success = matches!(result, Ok(OperationAnswer::LoadQuests(ref values)) if values.iter().all(|v| *v));
        match &mut self.cycle {
            Some(Cycle::Load { id, answer, .. }) if *id == answered => *answer = Some(success),
            Some(Cycle::Mutations {
                active: Some(active),
                ..
            }) => {
                if let Some(Preparation::Load { id, answer }) = &mut active.prepare
                    && *id == answered
                {
                    *answer = Some(success);
                }
            }
            _ => {}
        }
    }
    pub fn completed(&mut self, id: f64) {
        self.settling.retain(|m| m.intent.id != id);
        if let Some(Cycle::Mutations {
            active: Some(active),
            ..
        }) = &mut self.cycle
            && active.intent.id == id
        {
            active.completed = true;
        }
    }
    pub fn advance(
        &mut self,
        host: &impl Host,
        waker: &Waker,
        config: &EnvironmentState,
        logs: &mut Failures,
        now: Instant,
    ) -> Option<Instant> {
        self.settling.retain(|m| now < m.deadline);
        let cycle = self.advance_cycle(host, waker, config, logs, now);
        cycle
            .into_iter()
            .chain(self.settling.iter().map(|m| m.deadline))
            .min()
    }
    fn advance_cycle(
        &mut self,
        host: &impl Host,
        waker: &Waker,
        config: &EnvironmentState,
        logs: &mut Failures,
        now: Instant,
    ) -> Option<Instant> {
        self.unavailable
            .retain(|id| config.quest_ids.contains(&(*id as f64)));
        self.failures
            .retain(|(id, _), _| config.quest_ids.contains(&(*id as f64)));
        for _ in 0..64 {
            let Some(cycle) = self.cycle.take() else {
                let at = self.next?;
                if now < at {
                    return Some(at);
                }
                self.next = None;
                self.cycle = Some(Cycle::Snapshot(Read::new(host.snapshot(false), waker, now)));
                continue;
            };
            match cycle {
                Cycle::Snapshot(read) => match read.poll(now) {
                    None => {
                        let at = read.deadline;
                        self.cycle = Some(Cycle::Snapshot(read));
                        return Some(at);
                    }
                    Some(Err(_)) => {
                        logs.set("quests", false);
                        self.finish(now);
                    }
                    Some(Ok(snapshot)) => {
                        if !snapshot.logged_in {
                            self.finish(now);
                            continue;
                        }
                        let missing: Vec<_> = config
                            .quest_ids
                            .iter()
                            .filter(|id| !snapshot.quests.contains(id))
                            .copied()
                            .collect();
                        if missing.is_empty() {
                            logs.set("quests/load", true);
                            self.intents(host, waker, now);
                        } else {
                            match host.load(missing) {
                                Ok(id) => {
                                    self.cycle = Some(Cycle::Load {
                                        id,
                                        deadline: now + TIMEOUT,
                                        answer: None,
                                    })
                                }
                                Err(_) => {
                                    logs.set("quests/load", false);
                                    self.intents(host, waker, now);
                                }
                            }
                        }
                    }
                },
                Cycle::Load {
                    id,
                    deadline,
                    answer,
                } => {
                    if answer.is_none() && now < deadline {
                        self.cycle = Some(Cycle::Load {
                            id,
                            deadline,
                            answer,
                        });
                        return Some(deadline);
                    }
                    if answer.is_none() {
                        host.cancel(id);
                    }
                    logs.set("quests/load", answer == Some(true));
                    self.intents(host, waker, now);
                }
                Cycle::Targets(read) => match read.poll(now) {
                    None => {
                        let at = read.deadline;
                        self.cycle = Some(Cycle::Targets(read));
                        return Some(at);
                    }
                    Some(result) => {
                        logs.set("quests/drop-targets", result.is_ok());
                        self.cycle = Some(Cycle::Intents {
                            queue: config.quest_ids.clone().into(),
                            probes: vec![],
                            intents: vec![],
                        });
                    }
                },
                Cycle::Intents {
                    mut queue,
                    mut probes,
                    mut intents,
                } => {
                    while probes.len() < 4 {
                        let Some(id) = queue.pop_front() else {
                            break;
                        };
                        probes.push(Probe {
                            id,
                            progress: None,
                            read: call(
                                host,
                                waker,
                                now,
                                "quests.isInProgress",
                                vec![Value::Number(id)],
                            ),
                        });
                    }
                    let mut waiting = Vec::new();
                    for mut probe in probes {
                        if !config.quest_ids.contains(&probe.id) {
                            continue;
                        }
                        let Some(result) = probe.read.poll(now) else {
                            waiting.push(probe);
                            continue;
                        };
                        let key = format!("quests/{}/reconcile", probe.id);
                        match result.and_then(boolean) {
                            Err(_) => logs.set(&key, false),
                            Ok(value) => match probe.progress {
                                None => {
                                    probe.progress = Some(value);
                                    probe.read = call(
                                        host,
                                        waker,
                                        now,
                                        if value {
                                            "quests.canComplete"
                                        } else {
                                            "quests.isAvailable"
                                        },
                                        vec![Value::Number(probe.id)],
                                    );
                                    waiting.push(probe);
                                }
                                Some(progress) => {
                                    logs.set(&key, true);
                                    if progress || value {
                                        self.unavailable.remove(&(probe.id as i64));
                                    } else if self.unavailable.insert(probe.id as i64) {
                                        tracing::warn!(
                                            quest_id = probe.id,
                                            "environment quest is unavailable; skipping accept"
                                        );
                                    }
                                    if value {
                                        intents.push(Intent {
                                            id: probe.id,
                                            complete: progress,
                                        });
                                    }
                                }
                            },
                        }
                    }
                    if queue.is_empty() && waiting.is_empty() {
                        intents.sort_by(|a, b| a.id.total_cmp(&b.id));
                        self.cycle = Some(Cycle::Mutations {
                            queue: intents.into(),
                            active: None,
                        });
                    } else {
                        let at = if waiting.len() < 4 && !queue.is_empty() {
                            now
                        } else {
                            waiting.iter().map(|p| p.read.deadline).min().unwrap_or(now)
                        };
                        self.cycle = Some(Cycle::Intents {
                            queue,
                            probes: waiting,
                            intents,
                        });
                        return Some(at);
                    }
                }
                Cycle::Mutations {
                    mut queue,
                    mut active,
                } => {
                    if let Some(mutation) = &mut active {
                        if !config.quest_ids.contains(&mutation.intent.id) {
                            mutation.cancel(host);
                            if mutation.intent.complete && mutation.issued && !mutation.completed {
                                self.settling.push(active.take().unwrap());
                            } else {
                                active = None;
                            }
                        } else if let Some(outcome) = mutation.advance(host, waker, config, now) {
                            match outcome {
                                Outcome::Unavailable => {
                                    if self.unavailable.insert(mutation.intent.id as i64) {
                                        tracing::warn!(
                                            quest_id = mutation.intent.id,
                                            "environment quest is unavailable; skipping accept"
                                        );
                                    }
                                }
                                Outcome::Settled => {}
                                outcome => {
                                    let success = matches!(outcome, Outcome::Success);
                                    let intent = mutation.intent;
                                    let key = (intent.id as i64, intent.complete);
                                    logs.set(
                                        &format!(
                                            "quests/mutation/{}:{}",
                                            if intent.complete {
                                                "complete"
                                            } else {
                                                "accept"
                                            },
                                            intent.id
                                        ),
                                        success,
                                    );
                                    if success {
                                        self.failures.remove(&key);
                                    } else {
                                        let attempts = self
                                            .failures
                                            .get(&key)
                                            .map_or(1, |b| b.attempts.saturating_add(1));
                                        let seconds = match attempts {
                                            1 => 2,
                                            2 => 5,
                                            _ => 15,
                                        };
                                        self.failures.insert(
                                            key,
                                            Backoff {
                                                attempts,
                                                after: now + Duration::from_secs(seconds),
                                            },
                                        );
                                    }
                                }
                            }
                            active = None;
                        }
                    }
                    if active.is_none() {
                        while let Some(intent) = queue.front().copied() {
                            if !config.quest_ids.contains(&intent.id)
                                || self
                                    .failures
                                    .get(&(intent.id as i64, intent.complete))
                                    .is_some_and(|b| now < b.after)
                            {
                                queue.pop_front();
                                continue;
                            }
                            if let Some(at) = self
                                .last_mutation
                                .map(|at| at + SPACING)
                                .filter(|at| now < *at)
                            {
                                self.cycle = Some(Cycle::Mutations { queue, active });
                                return Some(at);
                            }
                            queue.pop_front();
                            match host.claim(intent.id) {
                                Ok(Some(lease)) => {
                                    self.last_mutation = Some(now);
                                    active = Some(Mutation {
                                        intent,
                                        _lease: lease,
                                        stage: if intent.complete {
                                            Stage::Progress
                                        } else {
                                            Stage::Available
                                        },
                                        read: None,
                                        deadline: now + TIMEOUT,
                                        poll: now,
                                        completed: false,
                                        prepare: (!intent.complete).then(|| {
                                            Preparation::Snapshot(Read::new(
                                                host.snapshot(false),
                                                waker,
                                                now,
                                            ))
                                        }),
                                        action_ready: false,
                                        issued: false,
                                    });
                                    break;
                                }
                                Ok(None) => {}
                                Err(_) => logs.set("quests/claim", false),
                            }
                        }
                    }
                    if active.is_none() && queue.is_empty() {
                        logs.set("quests", true);
                        self.finish(now);
                    } else {
                        let at = active.as_ref().map_or(now, |a| {
                            if let Some(read) = &a.read {
                                a.deadline.min(read.deadline)
                            } else {
                                a.poll.min(a.deadline)
                            }
                        });
                        self.cycle = Some(Cycle::Mutations { queue, active });
                        return Some(at);
                    }
                }
            }
        }
        Some(now)
    }
    fn finish(&mut self, now: Instant) {
        self.next = Some(now + QUEST_INTERVAL);
    }
    fn intents(&mut self, host: &impl Host, waker: &Waker, now: Instant) {
        self.cycle = Some(Cycle::Targets(Read::new(host.snapshot(true), waker, now)));
    }
}
impl Mutation {
    fn cancel(&self, host: &impl Host) {
        if let Some(Preparation::Load { id, .. }) = self.prepare {
            host.cancel(id);
        }
    }

    fn advance(
        &mut self,
        host: &impl Host,
        waker: &Waker,
        config: &EnvironmentState,
        now: Instant,
    ) -> Option<Outcome> {
        if self.completed && self.intent.complete {
            return Some(Outcome::Success);
        }
        if now >= self.deadline {
            self.cancel(host);
            return Some(Outcome::Failed);
        }
        if now < self.poll && self.prepare.is_none() {
            return None;
        }
        if matches!(self.stage, Stage::Event) {
            self.poll = self.deadline;
            return None;
        }
        if let Some(prepare) = self.prepare.take() {
            match prepare {
                Preparation::Snapshot(read) => match read.poll(now) {
                    None => {
                        self.poll = read.deadline;
                        self.prepare = Some(Preparation::Snapshot(read));
                        return None;
                    }
                    Some(Ok(snapshot)) if snapshot.quests.contains(&self.intent.id) => {}
                    Some(Ok(_)) => {
                        self.prepare = Some(Preparation::Action(call(
                            host,
                            waker,
                            now,
                            "world.isActionAvailable",
                            vec![Value::String("acceptQuest".into())],
                        )))
                    }
                    Some(Err(_)) => return Some(Outcome::Failed),
                },
                Preparation::Action(read) => match read.poll(now) {
                    None => {
                        self.poll = read.deadline;
                        self.prepare = Some(Preparation::Action(read));
                        return None;
                    }
                    Some(result) => match result.and_then(boolean) {
                        Ok(true) => match host.load(vec![self.intent.id]) {
                            Ok(id) => {
                                self.action_ready = true;
                                self.prepare = Some(Preparation::Load { id, answer: None });
                            }
                            Err(_) => return Some(Outcome::Failed),
                        },
                        Ok(false) => {
                            self.poll = now + POLL;
                            self.prepare = Some(Preparation::Snapshot(Read::new(
                                host.snapshot(false),
                                waker,
                                now,
                            )));
                        }
                        Err(_) => return Some(Outcome::Failed),
                    },
                },
                Preparation::Load { id, answer } => match answer {
                    None => {
                        self.prepare = Some(Preparation::Load { id, answer });
                        self.poll = self.deadline;
                        return None;
                    }
                    Some(false) => return Some(Outcome::Failed),
                    Some(true) => {}
                },
            }
            self.poll = now;
            return None;
        }
        let id = self.intent.id;
        if self.read.is_none() {
            let (method, args) = match self.stage {
                Stage::Available => ("quests.isAvailable", vec![Value::Number(id)]),
                Stage::Progress | Stage::Verify => ("quests.isInProgress", vec![Value::Number(id)]),
                Stage::Completable => ("quests.canComplete", vec![Value::Number(id)]),
                Stage::Action => (
                    "world.isActionAvailable",
                    vec![Value::String(
                        if self.intent.complete {
                            "tryQuestComplete"
                        } else {
                            "acceptQuest"
                        }
                        .into(),
                    )],
                ),
                Stage::Maximum => ("quests.getMaxTurnIns", vec![Value::Number(id)]),
                Stage::Dispatch(turn_ins) => {
                    if self.intent.complete {
                        (
                            "quests.complete",
                            vec![
                                Value::Number(id),
                                Value::Number(turn_ins),
                                Value::Number(
                                    config
                                        .quest_rewards
                                        .get(&id.to_string())
                                        .copied()
                                        .unwrap_or(-1.),
                                ),
                                Value::Bool(false),
                            ],
                        )
                    } else {
                        ("quests.accept", vec![Value::Number(id), Value::Bool(true)])
                    }
                }
                Stage::Event => unreachable!(),
            };
            self.issued |= matches!(self.stage, Stage::Dispatch(_));
            self.read = Some(call(host, waker, now, method, args));
        }
        let result = self.read.as_ref().unwrap().poll(now)?;
        self.read = None;
        let Ok(value) = result else {
            return Some(Outcome::Failed);
        };
        match self.stage {
            Stage::Maximum => {
                self.stage = Stage::Dispatch(
                    lucent_protocol::bridge::to_json(value)
                        .as_ref()
                        .and_then(crate::projection::wire::wire_int)
                        .unwrap_or(1.)
                        .max(1.),
                )
            }
            Stage::Dispatch(_) if self.intent.complete => self.stage = Stage::Event,
            stage => {
                let Ok(value) = boolean(value) else {
                    return Some(Outcome::Failed);
                };
                match stage {
                    Stage::Available => {
                        if value {
                            self.stage = Stage::Progress;
                        } else {
                            return Some(Outcome::Unavailable);
                        }
                    }
                    Stage::Progress => {
                        if self.intent.complete {
                            if !value {
                                return Some(Outcome::Settled);
                            }
                            self.stage = Stage::Completable;
                        } else {
                            if value {
                                return Some(Outcome::Success);
                            }
                            self.stage = if self.action_ready {
                                Stage::Dispatch(1.)
                            } else {
                                Stage::Action
                            };
                        }
                    }
                    Stage::Completable => {
                        if !value {
                            return Some(Outcome::Settled);
                        }
                        self.stage = Stage::Action;
                    }
                    Stage::Action => {
                        if value {
                            self.stage = if self.intent.complete {
                                Stage::Maximum
                            } else {
                                Stage::Dispatch(1.)
                            };
                        } else {
                            self.poll = now + POLL;
                        }
                    }
                    Stage::Dispatch(_) => {
                        if !value {
                            return Some(Outcome::Failed);
                        }
                        self.stage = Stage::Verify;
                    }
                    Stage::Verify => {
                        if value {
                            return Some(Outcome::Success);
                        } else {
                            self.poll = now + POLL;
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        None
    }
}
