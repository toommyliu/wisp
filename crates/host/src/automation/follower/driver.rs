use super::{
    Action, CombatEvent, Config, Effect, Event, Follower, Location, LocationKind, Player, Position,
    Reply, RequestId, Snapshot,
};
use crate::{
    ScriptLink,
    bridge::{
        CallGuard, Error, EventInterests, EventKind, Pending, PendingCall, ScriptEvent, Value,
    },
    game::{
        jobs::{JobId, JobKind},
        profile::ProfileJob,
        travel,
    },
    sessions::{
        claims::Binding,
        follower::{Edit, Shared},
        operations::Packets,
    },
};
use lucent_protocol::{
    combat_profile::normalize_combat_profile,
    event::{LocationChange, ProjectionEvent},
    follower::{FollowerAttackTarget, FollowerConfig},
    query::MonsterSelector,
};
use std::{
    collections::VecDeque,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Waker,
    time::{Duration, Instant},
};

pub struct FollowerPolicy {
    link: ScriptLink,
    shared: Arc<Shared>,
    waker: Waker,
    origin: Instant,
    core: Follower,
    starting: bool,
    listening: bool,
    replies: Vec<crate::bridge::Reply<String>>,
    work: Option<(RequestId, Work)>,
    combat: Option<(RequestId, JobId)>,
    deadline: Option<u64>,
    backlog: VecDeque<Input>,
}
enum Input {
    Link(ScriptEvent),
    Overflow,
    Combat(JobId, CombatEvent),
    Done(RequestId, Reply),
}
enum Work {
    Travel(JobId, Pending<travel::Outcome>),
    GoTo(PendingCall),
    Walk(Position, PendingCall),
    Arriving(Position, u64),
}
#[derive(Default)]
struct View {
    snapshot: Snapshot,
    map: String,
    alive: bool,
}
struct Unchanged {
    packets: Arc<Packets>,
    binding: Binding,
}
impl CallGuard for Unchanged {
    fn issue(&mut self, waiting: &AtomicBool) -> Result<(), Error> {
        if waiting.load(Ordering::Acquire) && self.packets.binding() == self.binding {
            Ok(())
        } else {
            Err(Error::Stopped)
        }
    }
    fn returned(&mut self, _: &Result<Value, Error>) {}
}

impl From<&FollowerConfig> for Config {
    fn from(config: &FollowerConfig) -> Self {
        Self {
            target_name: config.target_name.clone(),
            combat_enabled: config.combat_enabled,
            copy_walk: config.copy_walk,
            retry_enabled: config.retry_enabled,
            max_attempts: NonZeroU32::new(config.max_attempts).unwrap_or(NonZeroU32::MIN),
            attack_priority: config.attack_priority.clone(),
            locked_zone_fallbacks: config.locked_zone_fallbacks.clone(),
            locked_zone_room_override: config.locked_zone_room_override.clone(),
        }
    }
}

impl FollowerPolicy {
    pub(crate) const INTERESTS: EventInterests = EventInterests::new(&[
        EventKind::Readiness,
        EventKind::Connection,
        EventKind::Invalidated,
        EventKind::JoinMap,
        EventKind::PlayersChanged,
        EventKind::PlayerLocation,
        EventKind::ServerMessage,
    ]);
    pub fn new(link: ScriptLink, waker: Waker) -> Self {
        let shared = link.records.follower.clone();
        shared.set_waker(waker.clone());
        link.set_waker(Waker::noop().clone());
        let records = shared.records.read().clone();
        let mut core = Follower::new((&records.config).into());
        core.restore(records.status.clone());
        Self {
            link,
            shared,
            waker,
            origin: Instant::now(),
            core,
            starting: records.status.enabled,
            listening: false,
            replies: Vec::new(),
            work: None,
            combat: None,
            deadline: None,
            backlog: VecDeque::new(),
        }
    }
    fn ms(&self, at: Instant) -> u64 {
        at.saturating_duration_since(self.origin)
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
    pub fn advance(&mut self, now: Instant) -> Option<Instant> {
        if self.link.player_status().is_stopped() {
            return None;
        }
        if !self.starting
            && !self.core.status().running
            && !self.shared.has_pending()
            && self.replies.is_empty()
        {
            self.shared.take_combat();
            return None;
        }
        let ms = self.ms(now);
        while let Some(mut request) = self.shared.take() {
            if request.alive(Instant::now()) {
                self.edit(&request.edit, ms);
                self.replies.extend(request.reply.take());
            }
        }
        let more = self.listening && self.collect(ms);
        let Some(view) = self.view() else {
            return Some(now + Duration::from_millis(1));
        };
        if std::mem::take(&mut self.starting) {
            self.listen();
            #[cfg(feature = "testing")]
            self.probe("start", ms);
            self.feed(&view, Event::Start, ms);
        }
        while let Some(input) = self.backlog.pop_front() {
            self.input(&view, input, ms);
        }
        if let Some((id, Work::Arriving(target, until))) = &self.work {
            let arrived = view.snapshot.me.as_ref().is_some_and(|me| {
                me.position
                    .is_some_and(|at| at.x == target.x && at.y == target.y)
            });
            if arrived || ms >= *until {
                let id = *id;
                self.work = None;
                self.feed(
                    &view,
                    Event::Completed {
                        id,
                        reply: Reply::Returned(arrived),
                    },
                    ms,
                );
            }
        }
        if self.deadline.is_some_and(|at| ms >= at) {
            self.feed(&view, Event::Tick, ms);
        }
        self.publish();
        if !self.replies.is_empty() {
            let records = serde_json::to_string(&*self.shared.records.read()).unwrap();
            for reply in self.replies.drain(..) {
                let _ = reply.send(Ok(records.clone()));
            }
        }
        if self.listening && !self.core.status().running {
            self.listening = false;
            self.link.set_waker(Waker::noop().clone());
        }
        let arriving = match self.work {
            Some((_, Work::Arriving(_, until))) => Some(until),
            _ => None,
        };
        let deadline = self
            .deadline
            .into_iter()
            .chain(arriving)
            .min()
            .map(|at| self.origin + Duration::from_millis(at));
        if more { Some(now) } else { deadline }
    }
    fn edit(&mut self, edit: &Edit, ms: u64) {
        match edit {
            Edit::Configure(config) => {
                if !self.core.status().running && !self.starting {
                    let config = config.clone().normalized();
                    self.core.configure((&config).into());
                    let mut records = self.shared.records.read().clone();
                    records.config = config;
                    records.profile = None;
                    self.shared.changes.set(&self.shared.records, records);
                }
            }
            Edit::Start { config, profile } => {
                self.feed(&View::default(), Event::Stop, ms);
                let config = config.clone().normalized();
                self.core.configure((&config).into());
                let mut records = self.shared.records.read().clone();
                records.config = config;
                records.profile = Some(normalize_combat_profile(profile));
                self.shared.changes.set(&self.shared.records, records);
                self.starting = true;
            }
            Edit::Stop => {
                self.starting = false;
                self.feed(&View::default(), Event::Stop, ms);
            }
        }
    }
    fn listen(&mut self) {
        if self.listening {
            return;
        }
        for _ in 0..=crate::bridge::EVENT_CAPACITY {
            if matches!(self.link.try_event(), Ok(None) | Err(Error::Stopped)) {
                break;
            }
        }
        self.listening = true;
        self.link.set_waker(self.waker.clone());
    }
    fn view(&self) -> Option<View> {
        let target = self.core.target_name().to_owned();
        self.link
            .read_for(Duration::ZERO, |projection| {
                let map = projection.world.map.name.clone();
                let player = |data: lucent_protocol::world::PlayerData| Player {
                    username: data.username,
                    location: Location {
                        map: map.clone(),
                        cell: data.entity.cell,
                        pad: data.pad,
                    },
                    position: Some(Position {
                        x: data.position.x,
                        y: data.position.y,
                    }),
                };
                let me = projection.world.get_me().map(|me| me.data());
                View {
                    alive: me.as_ref().is_some_and(|me| me.entity.alive()),
                    snapshot: Snapshot {
                        can_act: projection.readiness.can_act(),
                        me: me.map(player),
                        target: projection
                            .world
                            .get_player(&target)
                            .map(|target| player(target.data())),
                        random_room: Some(travel::random_private_room()),
                    },
                    map,
                }
            })
            .ok()
    }
    fn collect(&mut self, ms: u64) -> bool {
        let mut more = true;
        for _ in 0..64 {
            match self.link.try_event() {
                Ok(Some(event)) => self.backlog.push_back(Input::Link(event)),
                Err(Error::Overflow) => self.backlog.push_back(Input::Overflow),
                Ok(None) | Err(_) => {
                    more = false;
                    break;
                }
            }
        }
        for (job, event) in self.shared.take_combat() {
            self.backlog.push_back(Input::Combat(job, event));
        }
        let mut arriving = None;
        let reply = match &self.work {
            Some((_, Work::Travel(_, pending))) => pending
                .try_recv()
                .map(|outcome| Reply::Returned(outcome == Ok(travel::Outcome::Returned(true)))),
            Some((_, Work::GoTo(pending))) => pending.try_recv().map(|_| Reply::Returned(true)),
            Some((_, Work::Walk(target, pending))) => match pending.try_recv() {
                Some(Ok(Value::Bool(true))) => {
                    arriving = Some(Work::Arriving(*target, ms.saturating_add(3000)));
                    None
                }
                Some(_) => Some(Reply::Returned(false)),
                None => None,
            },
            Some((_, Work::Arriving(..))) | None => None,
        };
        if let (Some(arriving), Some((_, work))) = (arriving, &mut self.work) {
            *work = arriving;
        }
        if let Some(reply) = reply {
            let (id, _) = self.work.take().unwrap();
            self.backlog.push_back(Input::Done(id, reply));
        }
        more
    }
    fn input(&mut self, view: &View, input: Input, ms: u64) {
        let event = match input {
            Input::Overflow => Event::ReadinessChanged,
            Input::Done(id, reply) => Event::Completed { id, reply },
            Input::Combat(job, event) => match self.combat {
                Some((id, current)) if current == job => Event::Combat { id, event },
                _ => return,
            },
            Input::Link(ScriptEvent::Readiness { .. } | ScriptEvent::Invalidated) => {
                Event::ReadinessChanged
            }
            Input::Link(ScriptEvent::ServerMessage { command, message }) => {
                Event::ServerMessage { command, message }
            }
            Input::Link(ScriptEvent::Loader { event, projected }) => {
                if event.name == "onConnection" {
                    self.feed(view, Event::ConnectionChanged, ms);
                }
                for event in projected {
                    let event = match event {
                        ProjectionEvent::JoinMap { .. } => Event::OwnMapJoined,
                        ProjectionEvent::PlayersChanged => Event::PlayersChanged,
                        ProjectionEvent::PlayerLocation {
                            cell,
                            pad,
                            position,
                            username,
                            change,
                            ..
                        } => Event::TargetLocation {
                            username,
                            location: Location {
                                map: view.map.clone(),
                                cell,
                                pad,
                            },
                            kind: match change {
                                LocationChange::Cell => LocationKind::Cell,
                                LocationChange::Position => LocationKind::Position(Position {
                                    x: position.x,
                                    y: position.y,
                                }),
                                LocationChange::Walk { destination } => {
                                    LocationKind::Walk(Some(Position {
                                        x: destination.x,
                                        y: destination.y,
                                    }))
                                }
                            },
                        },
                        _ => continue,
                    };
                    self.feed(view, event, ms);
                }
                return;
            }
            Input::Link(_) => return,
        };
        self.feed(view, event, ms);
    }
    fn feed(&mut self, view: &View, event: Event, ms: u64) {
        let mut events = VecDeque::from([event]);
        while let Some(event) = events.pop_front() {
            let effects = self.core.advance(&view.snapshot, event, ms);
            self.deadline = effects.next_deadline;
            for effect in effects.actions {
                #[cfg(feature = "testing")]
                if let Effect::Issue { action, .. } = &effect {
                    self.probe(
                        match action {
                            Action::Travel(travel::Request::Join { .. }) => "join",
                            Action::Travel(travel::Request::Jump { .. }) => "jump",
                            Action::Travel(travel::Request::Exit) => "exit",
                            Action::GoTo(_) => "goto",
                            Action::Walk(_) => "walk",
                        },
                        ms,
                    );
                }
                self.apply(view, effect, &mut events);
            }
        }
    }
    #[cfg(feature = "testing")]
    fn probe(&self, name: &str, ms: u64) {
        if let Some(probe) = self.link.records.jobs.follower_probe.lock().clone() {
            probe(name, self.origin + Duration::from_millis(ms));
        }
    }
    fn apply(&mut self, view: &View, effect: Effect, events: &mut VecDeque<Event>) {
        match effect {
            Effect::Issue { id, action } => match self.issue(view, action) {
                Some(work) => self.work = Some((id, work)),
                None => events.push_back(Event::Completed {
                    id,
                    reply: Reply::Returned(false),
                }),
            },
            Effect::Cancel { id } => match self.work.take() {
                Some((current, Work::Travel(job, _))) if current == id => {
                    self.link.jobs.cancel(job)
                }
                Some((current, _)) if current == id => {}
                work => self.work = work,
            },
            Effect::StartCombat {
                id,
                priorities,
                fallback_rank,
            } => {
                let profile = self
                    .shared
                    .records
                    .read()
                    .profile
                    .clone()
                    .unwrap_or_else(|| normalize_combat_profile(&serde_json::Value::Null));
                let selectors = priorities
                    .into_iter()
                    .map(|target| match target {
                        FollowerAttackTarget::Id(id) => MonsterSelector::MonsterMapId {
                            monster_map_id: id as f64,
                        },
                        FollowerAttackTarget::Name(name) => MonsterSelector::Name(name),
                    })
                    .collect();
                let result = ProfileJob::new(profile, Some(fallback_rank))
                    .map_err(|limit| format!("combat profile exceeds native {limit:?} limit"))
                    .and_then(|executor| {
                        self.link.jobs.submit(
                            executor,
                            selectors,
                            JobKind::Follower(self.shared.clone()),
                        )
                    });
                match result {
                    Ok(job) => self.combat = Some((id, job)),
                    Err(_) if self.link.player_status().is_stopped() => {}
                    Err(error) => events.push_back(Event::Combat {
                        id,
                        event: CombatEvent::Failed(error),
                    }),
                }
            }
            Effect::EngageCombat { id, engaged } => {
                if let Some((_, job)) = self.combat.filter(|(current, _)| *current == id) {
                    self.link.jobs.engage(job, engaged);
                }
            }
            Effect::StopCombat => {
                if let Some((_, job)) = self.combat.take() {
                    self.link.jobs.cancel(job);
                }
                self.link
                    .inbox()
                    .post(crate::player::Msg::StopProfile(self.link.packets.binding()));
            }
        }
    }
    fn issue(&self, view: &View, action: Action) -> Option<Work> {
        match action {
            Action::Travel(request) => {
                let executor = travel::TravelJob::new(request).ok()?;
                let (reply, pending) = Pending::channel();
                pending.set_waker(self.waker.clone());
                let job = self.link.jobs.submit_travel(executor, reply.into()).ok()?;
                Some(Work::Travel(job, pending))
            }
            Action::GoTo(name) => Some(Work::GoTo(
                self.call("player.goToPlayer", vec![Value::String(name)]),
            )),
            Action::Walk(_) if !view.alive => None,
            Action::Walk(position) => {
                let target = Position {
                    x: position.x.trunc(),
                    y: position.y.trunc(),
                };
                let args = vec![Value::Number(target.x), Value::Number(target.y)];
                Some(Work::Walk(target, self.call("player.walkTo", args)))
            }
        }
    }
    fn call(&self, name: &str, args: Vec<Value>) -> PendingCall {
        let pending = self.link.inbox().call_loader_guarded(
            name.into(),
            args,
            Box::new(Unchanged {
                packets: self.link.packets.clone(),
                binding: self.link.packets.binding(),
            }),
        );
        pending.set_waker(self.waker.clone());
        pending
    }
    fn publish(&self) {
        if self.shared.records.read().status == *self.core.status() {
            return;
        }
        let mut records = self.shared.records.read().clone();
        records.status = self.core.status().clone();
        self.shared.changes.set(&self.shared.records, records);
    }
}

#[cfg(all(test, feature = "testing"))]
mod tests {
    use super::*;
    use crate::testing;
    use std::sync::atomic::AtomicUsize;

    struct Count(AtomicUsize);
    impl std::task::Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn emit(link: &ScriptLink, data: serde_json::Value) {
        link.call_loader(
            "probe.emit",
            vec![
                Value::String("onExtensionResponse".into()),
                Value::String(serde_json::json!({"type":"json","dataObj":data}).to_string()),
            ],
        )
        .recv_timeout(Duration::from_secs(15))
        .unwrap();
    }
    fn traffic(link: &ScriptLink, id: i64) {
        emit(
            link,
            serde_json::json!({"cmd":"uotls","unm":format!("player-{id}"),
                "o":{"entID":id,"strFrame":"r1","strPad":"Left","intHP":100,"intHPMax":100}}),
        );
        emit(
            link,
            serde_json::json!({"cmd":"uotls","unm":format!("player-{id}"),"o":{"tx":5,"ty":6}}),
        );
        link.call_loader("probe.echo", vec![])
            .recv_timeout(Duration::from_secs(15))
            .unwrap();
    }

    #[test]
    fn a_disabled_follower_takes_no_wakeups_or_deadlines_from_room_traffic() {
        let host = testing::headless_host();
        let (mut session, observer) = testing::start_probe(&host, testing::ProbeMovie::new());
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let mut policy = FollowerPolicy::new(
            session.attach(FollowerPolicy::INTERESTS).unwrap(),
            Waker::from(count.clone()),
        );
        assert_eq!(policy.advance(Instant::now()), None);
        for id in 10..20 {
            traffic(&observer, id);
        }
        assert_eq!(count.0.load(Ordering::SeqCst), 0);
        assert_eq!(policy.advance(Instant::now()), None);
        let started = observer.command(
            serde_json::from_value(serde_json::json!({"kind":"startFollower",
                "config":{"targetName":"player-a","combatEnabled":false,"copyWalk":false,
                    "retryEnabled":true,"maxAttempts":3,"selectedProfileId":"generic-base",
                    "attackPriority":[],"lockedZoneFallbacks":[],"lockedZoneRoomOverride":""},
                "profile":null}))
            .unwrap(),
        );
        let woken = count.0.load(Ordering::SeqCst);
        assert_eq!(woken, 1);
        assert!(policy.advance(Instant::now()).is_some());
        assert!(started.recv_timeout(Duration::from_secs(15)).is_ok());
        traffic(&observer, 30);
        assert!(count.0.load(Ordering::SeqCst) > woken);
    }
}
