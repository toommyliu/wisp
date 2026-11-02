pub(crate) mod prepare;

use super::profile::*;
use crate::{
    ScriptLink,
    bridge::{Error, EventInterests, EventKind, Pending, ScriptEvent},
    player::{Inbox, Msg},
    sessions::{
        AttachmentId,
        claims::{Binding, Owner},
        operations::{Endpoint, Shared},
    },
};
use lucent_protocol::{combat_profile::normalize_combat_profile, query::MonsterSelector};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::Waker,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct JobId(u64);
impl JobId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}
impl std::fmt::Display for JobId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
impl std::str::FromStr for JobId {
    type Err = std::num::ParseIntError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

const MAX_MESSAGES: usize = 16;
mod owner;
pub(crate) mod safe_move;
pub(crate) mod travel;
#[cfg(feature = "testing")]
pub(crate) use owner::{ActionProbe, ObservationProbe, TravelObservationProbe};
pub(crate) use owner::{JobGate, JobRequests, ScriptJobs};
use owner::{JobKind, JobRequest};

#[derive(Clone, Copy)]
pub(crate) struct JobClock {
    origin: Instant,
}
impl JobClock {
    pub(crate) fn ms(self, at: Instant) -> u64 {
        at.saturating_duration_since(self.origin)
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
    fn at(self, ms: u64) -> Instant {
        self.origin + Duration::from_millis(ms)
    }
}

pub(crate) struct Observation {
    pub(crate) binding: Binding,
    pub(crate) area: u64,
    pub(crate) snapshot: ProfileSnapshot,
}

#[derive(Clone)]
pub(crate) struct Dispatch {
    pub(crate) gate: Arc<JobGate>,
    pub(crate) cycle: Option<u64>,
    pub(crate) recovery: u64,
    pub(crate) binding: Binding,
    pub(crate) area: u64,
    pub(crate) action: Action,
    pub(crate) target: Option<TargetKey>,
    pub(crate) item: Option<i64>,
    #[cfg(feature = "testing")]
    pub(crate) probe: Option<ActionProbe>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DispatchReply {
    Accepted(bool),
    StaleTarget,
}

#[derive(Default)]
struct Seen {
    area: Option<u64>,
    next: u64,
    monsters: HashMap<i64, (u64, u64, u64)>,
}
impl Seen {
    fn begin(&mut self, area: u64) {
        if self.area != Some(area) {
            self.monsters.clear();
            self.area = Some(area);
        }
    }
    fn key(&mut self, area: u64, key: TargetKey) -> TargetKey {
        self.begin(area);
        let entry = self
            .monsters
            .entry(key.monster_map_id)
            .or_insert((u64::MAX, u64::MAX, 0));
        if (entry.0, entry.1) != (area, key.generation) {
            self.next = self
                .next
                .checked_add(1)
                .expect("profile incarnation exhausted");
            *entry = (area, key.generation, self.next);
        }
        TargetKey {
            generation: entry.2,
            ..key
        }
    }
    fn projected(&self, key: TargetKey) -> Option<TargetKey> {
        self.monsters
            .get(&key.monster_map_id)
            .filter(|entry| entry.2 == key.generation)
            .map(|entry| TargetKey {
                generation: entry.1,
                ..key
            })
    }
    fn death(&self, area: u64, key: TargetKey) -> Option<TargetKey> {
        self.monsters
            .get(&key.monster_map_id)
            .filter(|entry| entry.0 == area && entry.1 == key.generation)
            .map(|entry| TargetKey {
                generation: entry.2,
                ..key
            })
    }
}

pub(crate) struct ProfileJobs {
    link: ScriptLink,
    requests: Arc<JobRequests>,
    waker: Waker,
    clock: JobClock,
    jobs: Vec<Job>,
    travel: Vec<travel::Travel>,
    auto_attack: Option<JobId>,
    auto_attack_replies: Vec<(crate::bridge::Reply<String>, String)>,
}
struct Job {
    request: JobRequest,
    seen: Seen,
    snapshot: ProfileSnapshot,
    area: u64,
    waiting: Waiting,
    due: Option<Instant>,
    messages: VecDeque<(u64, Message)>,
    next: Option<ProfileEvent>,
    prepared: bool,
    reported_warnings: Vec<String>,
    recovering: bool,
    overflow_warned: bool,
    item: Option<i64>,
    observation_failure: Option<Instant>,
    done: bool,
}
enum Waiting {
    None,
    Observation {
        pending: Pending<Observation>,
        event: ProfileEvent,
        deadline: Instant,
    },
    Action {
        pending: Pending<DispatchReply>,
        command: Command,
        cycle: Option<u64>,
        recovery: u64,
        deadline: Instant,
    },
    Preparation(prepare::Preparing),
}

impl JobRequests {
    #[cfg(feature = "testing")]
    pub(crate) fn travel_len(&self) -> usize {
        self.handoff.lock().travel_live
    }
    #[cfg(feature = "testing")]
    pub(crate) fn has_controller(&self) -> bool {
        !self.controller_links.lock().is_empty()
    }

    #[cfg(feature = "testing")]
    pub(crate) fn len(&self) -> usize {
        self.handoff.lock().live
    }
}

impl ProfileJobs {
    pub(crate) const INTERESTS: EventInterests = EventInterests::new(&[
        EventKind::ProfileDeath,
        EventKind::UpdateMessage,
        EventKind::Operation,
        EventKind::OperationResponsibility,
        EventKind::OperationIssued,
        EventKind::Invalidated,
        EventKind::Connection,
    ]);
    pub(crate) fn new(link: ScriptLink, waker: Waker) -> Self {
        let requests = link.records.jobs.clone();
        requests.handoff.lock().waker = Some(waker.clone());
        link.records.auto_attack.set_waker(waker.clone());
        link.set_waker(Waker::noop().clone());
        Self {
            link,
            requests,
            waker,
            clock: JobClock {
                origin: Instant::now(),
            },
            jobs: Vec::new(),
            travel: Vec::new(),
            auto_attack: None,
            auto_attack_replies: Vec::new(),
        }
    }
    fn reconcile_auto_attack(&mut self, now: Instant) {
        let shared = self.link.records.auto_attack.clone();
        while let Some(mut request) = shared.take() {
            if !request.alive(now) {
                continue;
            }
            self.stop_auto_attack();
            let mut records = lucent_protocol::auto_attack::AutoAttackRecords::default();
            if let crate::sessions::auto_attack::Edit::Enable {
                profile,
                target_priority,
            } = &request.edit
            {
                records.config.enabled = true;
                records.config.profile = Some(normalize_combat_profile(profile));
                records.config.target_priority.clone_from(target_priority);
            }
            shared.changes.set(&shared.records, records);
            self.start_auto_attack();
            if let Some(reply) = request.reply.take() {
                self.auto_attack_replies.push((
                    reply,
                    serde_json::to_string(&*shared.records.read()).unwrap(),
                ));
            }
        }
        self.start_auto_attack();
    }
    fn stop_auto_attack(&mut self) {
        if let Some(id) = self.auto_attack.take() {
            if let Some(job) = self.jobs.iter_mut().find(|job| job.request.id == id) {
                job.cancel();
            }
            self.link.jobs.stop(id);
        }
    }
    fn start_auto_attack(&mut self) {
        if self.auto_attack.is_some() || self.link.player_status().is_stopped() {
            return;
        }
        let shared = self.link.records.auto_attack.clone();
        let mut records = shared.records.read().clone();
        if !records.config.enabled {
            return;
        }
        let profile = records
            .config
            .profile
            .clone()
            .unwrap_or_else(|| normalize_combat_profile(&serde_json::Value::Null));
        let priorities = records
            .config
            .target_priority
            .iter()
            .cloned()
            .map(MonsterSelector::Name)
            .collect();
        let result = ProfileJob::new(profile, Some(records.config.target_priority.len()))
            .map_err(|limit| format!("combat profile exceeds native {limit:?} limit"))
            .and_then(|executor| {
                self.link
                    .jobs
                    .submit(executor, priorities, JobKind::AutoAttack(shared.clone()))
            });
        match result {
            Ok(id) => {
                self.auto_attack = Some(id);
                records.status.running = true;
            }
            Err(_) if self.link.player_status().is_stopped() => {
                records.status.running = false;
            }
            Err(error) => {
                records.config.enabled = false;
                records.config.profile = None;
                records.status.running = false;
                records.status.last_error = Some(error);
            }
        }
        shared.changes.set(&shared.records, records);
    }
    pub(crate) fn advance(&mut self, now: Instant) -> Option<Instant> {
        let travel_deadline = self.advance_travel(now);
        self.reconcile_auto_attack(now);
        let requests = {
            self.requests
                .handoff
                .lock()
                .queue
                .drain(..)
                .collect::<Vec<_>>()
        };
        if self.jobs.is_empty() && !requests.is_empty() {
            for _ in 0..=crate::bridge::EVENT_CAPACITY {
                if matches!(self.link.try_event(), Ok(None) | Err(Error::Stopped)) {
                    break;
                }
            }
            self.link.set_waker(self.waker.clone());
        }
        for mut request in requests {
            if !Arc::ptr_eq(&request.player.packets, &self.link.packets)
                || request.detached.load(Ordering::Acquire)
                || !request.gate.open()
            {
                let mut handoff = self.requests.handoff.lock();
                handoff.live -= 1;
                handoff.scripts -= usize::from(matches!(request.kind, JobKind::Script));
                continue;
            }
            request
                .executor
                .set_engaged(request.gate.state.lock().engaged);
            self.jobs.push(Job {
                request,
                seen: Seen::default(),
                snapshot: ProfileSnapshot::default(),
                area: 0,
                waiting: Waiting::None,
                due: Some(now),
                messages: VecDeque::new(),
                next: Some(ProfileEvent::Tick),
                prepared: false,
                reported_warnings: Vec::new(),
                recovering: false,
                overflow_warned: false,
                item: None,
                observation_failure: None,
                done: false,
            });
        }
        for index in 0..64 {
            match self.link.try_event() {
                Ok(Some(event)) => self.event(event, now),
                Ok(None) => break,
                Err(Error::Overflow) => {
                    for job in &mut self.jobs {
                        job.recover(now, self.clock);
                    }
                    break;
                }
                Err(error) => {
                    for job in &mut self.jobs {
                        job.fail(&self.link.inbox(), error.to_string());
                    }
                    break;
                }
            }
            if index == 63 {
                return Some(now);
            }
        }
        for job in &mut self.jobs {
            job.turn(now, self.clock, &self.link, &self.waker);
        }
        let before = self.jobs.len();
        let finished_scripts = self
            .jobs
            .iter()
            .filter(|job| job.done && matches!(job.request.kind, JobKind::Script))
            .count();
        if self
            .jobs
            .iter()
            .any(|job| job.done && Some(job.request.id) == self.auto_attack)
        {
            self.auto_attack = None;
        }
        self.jobs.retain(|job| !job.done);
        let mut handoff = self.requests.handoff.lock();
        handoff.live -= before - self.jobs.len();
        handoff.scripts -= finished_scripts;
        drop(handoff);
        for (reply, record) in self.auto_attack_replies.drain(..) {
            let _ = reply.send(Ok(record));
        }
        if self.jobs.is_empty() {
            self.link.set_waker(Waker::noop().clone());
        }
        self.jobs
            .iter()
            .filter_map(|job| job.deadline())
            .chain(travel_deadline)
            .min()
    }
    fn event(&mut self, event: ScriptEvent, now: Instant) {
        for job in &mut self.jobs {
            match &event {
                ScriptEvent::ProfileDeath { area, key } => {
                    if let Some(key) = job.seen.death(*area, *key) {
                        let effect = job.request.executor.advance(
                            &job.snapshot,
                            ProfileEvent::MonsterDeath(key),
                            self.clock.ms(now),
                        );
                        if let Some(deadline) = effect.next_deadline {
                            job.due = Some(self.clock.at(deadline));
                        }
                    }
                }
                ScriptEvent::Loader { projected, .. } => {
                    for event in projected {
                        if let lucent_protocol::event::ProjectionEvent::UpdateMessage {
                            message,
                            monster_map_id,
                            source,
                            ..
                        } = event
                        {
                            let message = Message {
                                text: message.clone(),
                                monster_map_id: *monster_map_id,
                                source: match source {
                                    lucent_protocol::event::MessageSource::Animation => {
                                        MessageSource::Animation
                                    }
                                    lucent_protocol::event::MessageSource::Aura => {
                                        MessageSource::Aura
                                    }
                                },
                            };
                            if job
                                .request
                                .executor
                                .profile()
                                .message_triggers
                                .iter()
                                .flatten()
                                .any(|trigger| matches_message(trigger, &message))
                            {
                                job.queue_message(
                                    (job.request.executor.target_version(), message),
                                    false,
                                );
                            }
                        }
                    }
                }
                ScriptEvent::Invalidated => job.recover(now, self.clock),
                _ => {
                    if let Waiting::Preparation(preparing) = &mut job.waiting {
                        preparing.event(&event, now);
                    }
                }
            }
        }
    }
}
impl Drop for ProfileJobs {
    fn drop(&mut self) {
        let mut handoff = self.requests.handoff.lock();
        handoff.waker = None;
        handoff.live = 0;
        handoff.scripts = 0;
        handoff.queue.clear();
        for mut request in handoff.travel.drain(..) {
            request.gate.close();
            request
                .owner
                .complete(super::travel::Outcome::Cancelled, request.id);
        }
        handoff.travel_live = 0;
        for travel in &mut self.travel {
            travel.cancel();
        }
        for job in &mut self.jobs {
            job.cancel();
        }
    }
}

impl Job {
    fn queue_message(&mut self, message: (u64, Message), deferred: bool) {
        if deferred {
            self.messages.push_front(message);
        } else {
            self.messages.push_back(message);
        }
        if self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
            if !self.overflow_warned {
                self.overflow_warned = true;
                tracing::warn!(job = %self.request.id, "combat profile message queue overflowed; oldest message discarded");
            }
        }
    }
    fn recover(&mut self, now: Instant, clock: JobClock) {
        let binding = self.request.player.packets.binding();
        let reprepare = binding != self.request.binding
            || !self.prepared
            || matches!(self.waiting, Waiting::Preparation(_));
        {
            let mut gate = self.request.gate.state.lock();
            gate.binding = binding;
            gate.recovery += 1;
            gate.operation = None;
        }
        if let Waiting::Preparation(preparing) = &mut self.waiting {
            preparing.cancel();
        }
        if reprepare {
            self.request
                .player
                .packets
                .release_owner(self.request.owner);
            self.item = None;
        }
        self.request.binding = binding;
        self.waiting = Waiting::None;
        self.messages.clear();
        self.seen.monsters.clear();
        self.snapshot = ProfileSnapshot::default();
        self.request.executor.invalidate(reprepare, clock.ms(now));
        self.next = Some(ProfileEvent::Tick);
        self.due = Some(now);
        self.recovering = true;
    }
    fn deadline(&self) -> Option<Instant> {
        match &self.waiting {
            Waiting::Observation { deadline, .. } | Waiting::Action { deadline, .. } => {
                Some(*deadline)
            }
            Waiting::Preparation(preparing) => preparing.deadline(),
            Waiting::None if self.next.is_some() => match self.next {
                Some(ProfileEvent::Tick | ProfileEvent::Message { .. }) => self.due,
                _ => Some(Instant::now()),
            },
            Waiting::None if (self.request.executor.ready() && !self.messages.is_empty()) => {
                Some(Instant::now())
            }
            Waiting::None
                if !self.request.executor.ready() || self.request.gate.state.lock().engaged =>
            {
                self.due
            }
            Waiting::None => None,
        }
    }
    fn cancel(&mut self) {
        if matches!(self.request.kind, JobKind::AutoAttack(_)) && self.request.gate.close() {
            self.request
                .player
                .inbox
                .post(Msg::StopProfile(self.request.binding));
        }
        self.request.gate.state.lock().closed = true;
        self.request
            .player
            .packets
            .release_owner(self.request.owner);
        if let Waiting::Preparation(preparing) = &mut self.waiting {
            preparing.cancel();
        }
        self.done = true;
    }
    fn fail(&mut self, inbox: &Inbox, error: String) {
        if self.done {
            return;
        }
        if let JobKind::AutoAttack(shared) = &self.request.kind {
            if self.request.player.stopped.load(Ordering::Acquire) {
                self.cancel();
                return;
            }
            let mut records = shared.records.read().clone();
            records.config.enabled = false;
            records.config.profile = None;
            records.status.running = false;
            records.status.class_names.clear();
            records.status.warnings.clear();
            records.status.last_error = Some(error);
            shared.changes.set(&shared.records, records);
            self.cancel();
            return;
        }
        inbox.post(Msg::OperationAnswer(
            self.request.owner.attachment,
            ScriptEvent::JobFailed {
                id: self.request.id.0,
                error,
            },
        ));
        self.cancel();
    }
    fn prepared(&mut self, inbox: &Inbox) {
        if let JobKind::AutoAttack(shared) = &self.request.kind {
            let mut records = shared.records.read().clone();
            records.status.class_names = self
                .request
                .executor
                .profile()
                .class_names
                .clone()
                .unwrap_or_default();
            records.status.warnings = self.request.executor.warnings().to_vec();
            shared.changes.set(&shared.records, records);
            self.prepared = true;
            return;
        }
        if self.prepared {
            let warnings = self
                .request
                .executor
                .warnings()
                .iter()
                .filter(|warning| !self.reported_warnings.contains(warning))
                .cloned()
                .collect::<Vec<_>>();
            for warning in warnings {
                inbox.post(Msg::OperationAnswer(
                    self.request.owner.attachment,
                    ScriptEvent::JobWarning {
                        id: self.request.id.0,
                        warning: warning.clone(),
                    },
                ));
                self.reported_warnings.push(warning);
            }
            return;
        }
        self.prepared = true;
        self.reported_warnings = self.request.executor.warnings().to_vec();
        inbox.post(Msg::OperationAnswer(
            self.request.owner.attachment,
            ScriptEvent::JobPrepared {
                id: self.request.id.0,
                label: self.request.executor.profile().label.clone(),
                class_names: self
                    .request
                    .executor
                    .profile()
                    .class_names
                    .clone()
                    .unwrap_or_default(),
                warnings: self.request.executor.warnings().to_vec(),
            },
        ));
    }
    fn turn(&mut self, now: Instant, clock: JobClock, link: &ScriptLink, waker: &Waker) {
        if self.done {
            return;
        }
        if self.request.detached.load(Ordering::Acquire) || !self.request.gate.open() {
            self.cancel();
            return;
        }
        if self.request.player.packets.binding() != self.request.binding {
            self.recover(now, clock);
        }
        let engaged = self.request.gate.state.lock().engaged;
        self.request.executor.set_engaged(engaged);
        let mut waiting = std::mem::replace(&mut self.waiting, Waiting::None);
        match &mut waiting {
            Waiting::Preparation(preparing) => {
                match preparing.advance(now, &self.request, link, waker) {
                    Some(Ok(reply)) => {
                        if let Reply::Prepared { skill5_item_id, .. } = &reply {
                            self.item = *skill5_item_id;
                        }
                        self.next = Some(ProfileEvent::Completed {
                            id: preparing.action(),
                            reply,
                        });
                    }
                    Some(Err(error)) => {
                        self.fail(&link.inbox(), error.to_string());
                        return;
                    }
                    None => {
                        self.waiting = waiting;
                        return;
                    }
                }
            }
            Waiting::Observation {
                pending,
                event,
                deadline,
            } => match pending.try_recv() {
                Some(Ok(mut observation)) => {
                    if observation.binding != self.request.binding {
                        self.recover(now, clock);
                        return;
                    }
                    self.seen.begin(observation.area);
                    for monster in &mut observation.snapshot.monsters {
                        monster.key = self.seen.key(observation.area, monster.key);
                    }
                    if let SelectedTarget::Monster(key) = &mut observation.snapshot.selected {
                        *key = self.seen.key(observation.area, *key);
                    }
                    self.area = observation.area;
                    self.snapshot = observation.snapshot;
                    self.observation_failure = None;
                    self.recovering = false;
                    self.feed(event.clone(), now, clock, link, waker);
                    return;
                }
                Some(Err(error)) => {
                    let since = *self.observation_failure.get_or_insert(now);
                    if now.duration_since(since) >= Duration::from_secs(5) {
                        self.fail(
                            &link.inbox(),
                            format!("combat profile observation failed for 5 seconds: {error}"),
                        );
                        return;
                    }
                    self.next = Some(event.clone());
                    self.due = Some(now + Duration::from_millis(250));
                    return;
                }
                None if now < *deadline => {
                    self.waiting = waiting;
                    return;
                }
                None => {
                    self.fail(&link.inbox(), "combat profile observation timed out".into());
                    return;
                }
            },
            Waiting::Action {
                pending,
                command,
                cycle,
                recovery,
                deadline,
            } => match pending.try_recv() {
                Some(Ok(reply)) => {
                    self.next = Some(ProfileEvent::Completed {
                        id: command.id,
                        reply: match reply {
                            DispatchReply::StaleTarget => Reply::Abandoned {
                                target_lost: self.request.gate.allows(*cycle, *recovery),
                            },
                            DispatchReply::Accepted(value) => Reply::Accepted(value),
                        },
                    });
                }
                Some(Err(error)) => {
                    if matches!(self.request.kind, JobKind::AutoAttack(_)) {
                        self.fail(&link.inbox(), error.to_string());
                        return;
                    }
                    self.next = Some(ProfileEvent::Completed {
                        id: command.id,
                        reply: Reply::Accepted(false),
                    })
                }
                None if now < *deadline => {
                    self.waiting = waiting;
                    return;
                }
                None => {
                    self.next = Some(ProfileEvent::Completed {
                        id: command.id,
                        reply: Reply::Accepted(false),
                    })
                }
            },
            Waiting::None => {}
        }
        if self
            .next
            .as_ref()
            .is_some_and(|event| matches!(event, ProfileEvent::Tick | ProfileEvent::Message { .. }))
            && self.due.is_some_and(|due| now < due)
        {
            return;
        }
        let event = if let Some(event) = self.next.take() {
            event
        } else if self.request.executor.ready() && !self.messages.is_empty() {
            let (observed_version, message) = self.messages.pop_front().unwrap();
            ProfileEvent::Message {
                observed_version,
                message,
            }
        } else if (engaged || !self.request.executor.ready())
            && self.due.is_some_and(|due| now >= due)
        {
            ProfileEvent::Tick
        } else {
            return;
        };
        let observe = self.recovering
            || matches!(event, ProfileEvent::Message { .. })
            || (matches!(event, ProfileEvent::Tick) && self.request.executor.needs_observation());
        if observe {
            let state = self.request.gate.state.lock();
            if state.closed {
                return;
            }
            let (reply, pending) = Pending::channel();
            pending.set_waker(waker.clone());
            self.request.player.inbox.post(Msg::ObserveProfile {
                priorities: self.request.priorities.clone(),
                clock,
                reply,
                #[cfg(feature = "testing")]
                probe: link.records.jobs.observation_probe.lock().clone(),
            });
            drop(state);
            self.waiting = Waiting::Observation {
                pending,
                event,
                deadline: now + Duration::from_secs(5),
            };
        } else {
            self.feed(event, now, clock, link, waker);
        }
    }
    fn feed(
        &mut self,
        event: ProfileEvent,
        now: Instant,
        clock: JobClock,
        link: &ScriptLink,
        waker: &Waker,
    ) {
        let prepared = matches!(
            &event,
            ProfileEvent::Completed {
                reply: Reply::Prepared { .. },
                ..
            }
        );
        let effect = self
            .request
            .executor
            .advance(&self.snapshot, event.clone(), clock.ms(now));
        if prepared {
            self.due = Some(now);
        }
        if prepared
            || (self.request.executor.profile().steps.is_empty()
                && self
                    .request
                    .executor
                    .profile()
                    .message_triggers
                    .iter()
                    .flatten()
                    .next()
                    .is_none())
        {
            self.prepared(&link.inbox());
        }
        if let Some(deadline) = effect.next_deadline {
            self.due = Some(clock.at(deadline));
        }
        match effect.outcome {
            Some(Outcome::Deferred) => {
                if let ProfileEvent::Message {
                    observed_version,
                    message,
                } = event
                {
                    self.queue_message((observed_version, message), true);
                }
            }
            Some(Outcome::Failure { message, .. }) => {
                self.fail(&link.inbox(), message);
                return;
            }
            Some(Outcome::Cycle(Cycle::NoTarget))
                if matches!(self.request.kind, JobKind::Script) =>
            {
                let state = self.request.gate.state.lock();
                if !state.closed && state.engaged {
                    self.request
                        .player
                        .inbox
                        .post(Msg::StopProfile(self.request.binding));
                }
            }
            _ => {}
        }
        if let Some(command) = effect.command {
            self.due = None;
            match &command.action {
                Action::Prepare => {
                    self.waiting = Waiting::Preparation(prepare::Preparing::new(command.id))
                }
                Action::ReleasePreparation => self.cancel(),
                action => {
                    #[cfg(feature = "testing")]
                    if let Some(probe) = link.records.jobs.dispatch_probe.lock().clone() {
                        probe(self.request.id.0, matches!(action, Action::Attack(_)));
                    }
                    let state = self.request.gate.state.lock();
                    if state.closed {
                        return;
                    }
                    let cycle = match action {
                        Action::UseSkill {
                            force: CastForce::Forced,
                            ..
                        } => None,
                        _ => Some(state.epoch),
                    };
                    if cycle.is_some() && !state.engaged {
                        self.next = Some(ProfileEvent::Completed {
                            id: command.id,
                            reply: Reply::Abandoned { target_lost: false },
                        });
                        return;
                    }
                    let target = match action {
                        Action::Attack(lease) => Some(lease.key),
                        Action::UseSkill { target, .. } => target.map(|lease| lease.key),
                        _ => unreachable!(),
                    };
                    let (reply, pending) = Pending::channel();
                    pending.set_waker(waker.clone());
                    self.request.player.inbox.post(Msg::DispatchProfile(
                        Dispatch {
                            gate: self.request.gate.clone(),
                            cycle,
                            recovery: state.recovery,
                            binding: self.request.binding,
                            area: self.area,
                            action: action.clone(),
                            target: target.and_then(|key| self.seen.projected(key)),
                            item: self.item,
                            #[cfg(feature = "testing")]
                            probe: link.records.jobs.action_probe.lock().clone(),
                        },
                        reply,
                    ));
                    let recovery = state.recovery;
                    drop(state);
                    self.waiting = Waiting::Action {
                        pending,
                        command,
                        cycle,
                        recovery,
                        deadline: now + Duration::from_secs(5),
                    };
                }
            }
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod tests;

#[cfg(all(test, feature = "testing"))]
mod recovery_tests;
