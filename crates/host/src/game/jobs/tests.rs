use super::*;
use crate::bridge::{ScriptEvent, Value};
use crate::{Supervisor, testing};
use serde_json::json;
use std::sync::mpsc;
const TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn emit(link: &ScriptLink, data: serde_json::Value) {
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onExtensionResponse".into()),
            Value::String(json!({"type":"json","dataObj":data}).to_string()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}
fn area(link: &ScriptLink, id: i64) {
    emit(
        link,
        json!({"cmd":"moveToArea","areaId":id,"areaName":format!("test-{id}"),
        "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],
        "monBranch":[{"MonID":1,"MonMapID":2,"strMonName":"Boss","intHP":50,"intHPMax":50,"intState":1}]}),
    );
}
fn drain(link: &ScriptLink) -> Vec<String> {
    link.call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    let mut calls = Vec::new();
    while let Some(event) = link.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
            && let Some(Value::String(name)) = event.args.first()
        {
            calls.push(name.clone());
        }
    }
    calls
}
fn start(link: &ScriptLink) -> JobId {
    start_profile(link, json!({"steps":[{"skill":1}]}))
}
pub(super) fn start_profile(link: &ScriptLink, profile: serde_json::Value) -> JobId {
    let deadline = Instant::now() + TIMEOUT;
    let id = loop {
        match link
            .jobs
            .start(&json!({"profile":profile,"targetPriority":["Boss"]}).to_string())
        {
            Ok(id) => break id,
            Err(error) if error.contains("session controller") && Instant::now() < deadline => {
                std::thread::yield_now()
            }
            Err(error) => panic!("{error}"),
        }
    };
    link.jobs.engage(id, true);
    id
}

#[test]
fn stale_incarnations_make_no_loader_calls_before_a_fresh_attack() {
    for change in ["respawn", "area", "connection", "player"] {
        let host = testing::headless_host();
        let (mut session, mut observer) = testing::start_probe(
            &host,
            testing::ProbeMovie::new()
                .combat_facts()
                .constant("flash.getGameObjectS", json!("\"player-a\""))
                .constant("flash.callGameFunction0", json!("true"))
                .constant("world.getAvailableMonsterMapIds", json!([2]))
                .recording("world.getAvailableMonsterMapIds")
                .responding("combat.attackMonster", [(json!(true), vec![])])
                .responding("combat.useSkill", [(json!(true), vec![])]),
        );
        area(&observer, 1);
        let mut supervisor = Supervisor::start(&mut session, None).unwrap();
        let mut owner = session.attach(EventInterests::ALL).unwrap();
        let (entered, barrier) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let resume = Mutex::new(resume);
        let first = AtomicBool::new(true);
        session.set_profile_dispatch_probe(move |_, _| {
            if first.swap(false, Ordering::AcqRel) {
                entered.send(()).unwrap();
                resume.lock().recv_timeout(TIMEOUT).unwrap();
            }
        });
        let (completed, completion) = mpsc::channel();
        let (settle, settled) = mpsc::channel();
        let settled = Mutex::new(settled);
        let first_stale = AtomicBool::new(true);
        session.set_profile_action_probe(move |stale| {
            let _ = completed.send(stale);
            if stale && first_stale.swap(false, Ordering::AcqRel) {
                settled.lock().recv_timeout(TIMEOUT).unwrap();
            }
        });
        let id = start(&owner);
        barrier.recv_timeout(TIMEOUT).unwrap();
        match change {
            "respawn" => {
                emit(
                    &observer,
                    json!({"cmd":"cb","m":{"2":{"intHP":0,"intState":0}}}),
                );
                emit(
                    &observer,
                    json!({"cmd":"cb","m":{"2":{"intHP":50,"intState":1}}}),
                );
            }
            "area" => area(&observer, 2),
            "connection" => {
                observer
                    .call_loader(
                        "probe.emit",
                        vec![
                            Value::String("onConnection".into()),
                            Value::String("OnConnection".into()),
                        ],
                    )
                    .recv_timeout(TIMEOUT)
                    .unwrap();
                area(&observer, 1);
            }
            "player" => {
                supervisor
                    .restart_player(&mut session, testing::headless_surface())
                    .unwrap();
            }
            _ => unreachable!(),
        }
        if change != "player" {
            drain(&observer);
        }
        release.send(()).unwrap();
        if change != "player" {
            assert!(completion.recv_timeout(TIMEOUT).unwrap(), "{change}");
            owner.jobs.cancel(id);
            settle.send(()).unwrap();
            let calls = drain(&observer);
            assert!(
                calls.iter().all(|name| !matches!(
                    name.as_str(),
                    "combat.attackMonster" | "combat.useSkill"
                )),
                "{change}: {calls:?}"
            );
        } else {
            owner = session.attach(EventInterests::ALL).unwrap();
            observer = session.attach(EventInterests::ALL).unwrap();
            session.send(crate::ViewEvent::Resized(
                crate::Viewport::new(100.0, 100.0, 1.0).unwrap(),
            ));
            testing::wait_loaded(&observer);
            area(&observer, 1);
        }
        let fresh = start(&owner);
        assert!(!completion.recv_timeout(TIMEOUT).unwrap(), "{change}");
        owner.jobs.cancel(fresh);
        assert!(
            drain(&observer)
                .iter()
                .any(|name| name == "combat.attackMonster"),
            "{change}"
        );
    }
}

#[test]
fn old_deaths_do_not_match_a_new_incarnation() {
    let mut seen = Seen::default();
    let old = TargetKey {
        monster_map_id: 2,
        generation: 0,
    };
    let first = seen.key(1, old);
    let second = seen.key(
        1,
        TargetKey {
            generation: 1,
            ..old
        },
    );
    assert_ne!(first, second);
    assert_eq!(seen.death(1, old), None);
    assert_eq!(
        seen.death(
            1,
            TargetKey {
                generation: 1,
                ..old
            }
        ),
        Some(second)
    );
    let third = seen.key(2, old);
    assert_ne!(second, third);
    assert_eq!(
        seen.death(
            1,
            TargetKey {
                generation: 1,
                ..old
            }
        ),
        None
    );
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        testing::ProbeMovie::new()
            .constant("flash.getGameObjectS", json!("\"player-a\""))
            .constant("flash.callGameFunction0", json!("true")),
    );
    let deaths = session
        .attach(EventInterests::new(&[EventKind::ProfileDeath]))
        .unwrap();
    area(&observer, 1);
    emit(
        &observer,
        json!({"cmd":"cb","m":{"2":{"intHP":0,"intState":0}}}),
    );
    emit(
        &observer,
        json!({"cmd":"cb","m":{"2":{"intHP":50,"intState":1}}}),
    );
    let Some(ScriptEvent::ProfileDeath { area, key }) = deaths.try_event().unwrap() else {
        panic!("missing stamped death");
    };
    assert_eq!(key.generation, 0);
    let generation = observer
        .read(|state| state.world.get_monster(2).unwrap().generation())
        .unwrap();
    assert_eq!(generation, 1);
    let mut current = Seen::default();
    current.key(area, TargetKey { generation, ..key });
    assert_eq!(current.death(area, key), None);
}

#[test]
fn a_queued_action_rechecks_revocation_on_the_player() {
    let host = testing::headless_host();
    let (session, observer) = testing::start_probe(
        &host,
        testing::ProbeMovie::new()
            .combat_facts()
            .constant("flash.getGameObjectS", json!("\"player-a\""))
            .constant("flash.callGameFunction0", json!("true"))
            .constant("world.getAvailableMonsterMapIds", json!([2]))
            .recording("world.getAvailableMonsterMapIds")
            .responding("combat.attackMonster", [(json!(true), vec![])]),
    );
    area(&observer, 1);
    let (reply, observation) = Pending::channel();
    observer.inbox().post(Msg::ObserveProfile {
        priorities: Arc::from([]),
        clock: JobClock {
            origin: Instant::now(),
        },
        reply,
        probe: None,
    });
    let observation = observation.recv_timeout(TIMEOUT).unwrap();
    let gate = Arc::new(JobGate::new(observation.binding));
    gate.state.lock().engaged = true;
    let key = observation.snapshot.monsters[0].key;
    drain(&observer);
    let (entered, barrier) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let blocked = session.read_projection(move |_| {
        entered.send(()).unwrap();
        resume.recv_timeout(TIMEOUT).unwrap();
    });
    barrier.recv_timeout(TIMEOUT).unwrap();
    let (reply, action) = Pending::channel();
    observer.inbox().post(Msg::DispatchProfile(
        Dispatch {
            gate: gate.clone(),
            cycle: Some(0),
            recovery: 0,
            binding: observation.binding,
            area: observation.area,
            action: Action::Attack(TargetLease { key, version: 1 }),
            target: Some(key),
            item: None,
            probe: None,
        },
        reply,
    ));
    gate.close();
    release.send(()).unwrap();
    blocked.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(
        action.recv_timeout(TIMEOUT).unwrap(),
        DispatchReply::StaleTarget
    );
    assert!(drain(&observer).is_empty());
}

fn next_skill(link: &ScriptLink) -> String {
    struct Unpark(std::thread::Thread);
    impl std::task::Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    link.set_waker(Waker::from(Arc::new(Unpark(std::thread::current()))));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        assert!(Instant::now() < deadline, "no skill dispatched");
        match link.try_event().unwrap() {
            Some(ScriptEvent::Loader { event, .. })
                if event.name == "onProbeCall"
                    && event.args.first() == Some(&Value::String("combat.useSkill".into())) =>
            {
                let Value::List(args) = &event.args[1] else {
                    panic!("invalid skill args");
                };
                let Value::String(skill) = &args[0] else {
                    panic!("invalid skill slot");
                };
                return skill.clone();
            }
            Some(_) => {}
            None => std::thread::park_timeout(deadline.saturating_duration_since(Instant::now())),
        }
    }
}

#[test]
fn a_parked_unissued_cast_preserves_the_cursor() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        testing::ProbeMovie::new()
            .combat_facts()
            .constant("flash.getGameObjectS", json!("\"player-a\""))
            .constant("flash.callGameFunction0", json!("true"))
            .constant("world.getAvailableMonsterMapIds", json!([2]))
            .responding("combat.attackMonster", [(json!(true), vec![])])
            .responding("combat.useSkill", [(json!(true), vec![])]),
    );
    area(&observer, 1);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    let owner = session.attach(EventInterests::ALL).unwrap();
    let (entered, barrier) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let resume = Mutex::new(resume);
    let first = AtomicBool::new(true);
    session.set_profile_dispatch_probe(move |_, attack| {
        if !attack && first.swap(false, Ordering::AcqRel) {
            entered.send(()).unwrap();
            resume.lock().recv_timeout(TIMEOUT).unwrap();
        }
    });
    let id = start_profile(
        &owner,
        json!({"steps":[{"skill":1},{"skill":2}],"messageTriggers":[{"messageIncludes":"enrage","skill":4}]}),
    );
    barrier.recv_timeout(TIMEOUT).unwrap();
    owner.jobs.engage(id, false);
    emit(
        &observer,
        json!({"cmd":"cb","anims":[{"msg":"enrage","animStr":"cast","cInf":"m:2","tInf":"p:7"}]}),
    );
    release.send(()).unwrap();
    assert_eq!(next_skill(&observer), "4");
    owner.jobs.engage(id, true);
    assert_eq!(next_skill(&observer), "1");
    owner.jobs.cancel(id);
}

#[test]
fn a_stale_stamped_death_keeps_the_new_incarnation_target() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        testing::ProbeMovie::new()
            .combat_facts()
            .constant("flash.getGameObjectS", json!("\"player-a\""))
            .constant("flash.callGameFunction0", json!("true"))
            .constant("world.getAvailableMonsterMapIds", json!([2]))
            .responding("combat.attackMonster", [(json!(true), vec![])])
            .responding("combat.useSkill", [(json!(true), vec![])]),
    );
    area(&observer, 1);
    let link = session.attach(ProfileJobs::INTERESTS).unwrap();
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut jobs = ProfileJobs::new(link, waker);
    let owner = session.attach(EventInterests::ALL).unwrap();
    let id = start_profile(
        &owner,
        json!({"delayMs":50,"resetSkillIndexOnTargetDeath":true,"steps":[{"skill":1},{"skill":2}]}),
    );
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    emit(
        &observer,
        json!({"cmd":"cb","m":{"2":{"intHP":0,"intState":0}}}),
    );
    emit(
        &observer,
        json!({"cmd":"cb","m":{"2":{"intHP":50,"intState":1}}}),
    );
    let mut stale = None;
    while let Some(event) = jobs.link.try_event().unwrap() {
        if matches!(event, ScriptEvent::ProfileDeath { .. }) {
            stale = Some(event);
        }
    }
    let stale = stale.expect("missing stamped death");
    let calls = drive(&mut jobs, &observer, |_, calls| {
        calls
            .iter()
            .position(|name| name == "combat.attackMonster")
            .is_some_and(|at| calls[at..].iter().any(|name| name == "combat.useSkill"))
    });
    assert!(
        calls.iter().any(|name| name == "combat.attackMonster"),
        "{calls:?}"
    );
    assert_eq!(
        jobs.jobs[0].seen.monsters.get(&2).map(|entry| entry.1),
        Some(1),
        "the job has not observed the new incarnation"
    );
    let version = jobs.jobs[0].request.executor.target_version();
    jobs.event(stale, Instant::now());
    assert_eq!(
        jobs.jobs[0].request.executor.target_version(),
        version,
        "a death stamped for the old incarnation dropped the new target"
    );
    owner.jobs.cancel(id);
}

struct Unpark(std::thread::Thread);
impl std::task::Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn drive(
    jobs: &mut ProfileJobs,
    observer: &ScriptLink,
    until: impl Fn(&ProfileJobs, &[String]) -> bool,
) -> Vec<String> {
    let deadline = Instant::now() + TIMEOUT;
    let mut calls = Vec::new();
    loop {
        assert!(Instant::now() < deadline, "calls: {calls:?}");
        let next = jobs.advance(Instant::now());
        while let Some(event) = observer.try_event().unwrap() {
            if let ScriptEvent::Loader { event, .. } = event
                && event.name == "onProbeCall"
                && let Some(Value::String(name)) = event.args.first()
            {
                calls.push(name.clone());
            }
        }
        if until(jobs, &calls) {
            return calls;
        }
        std::thread::park_timeout(
            next.unwrap_or(deadline)
                .min(deadline)
                .saturating_duration_since(Instant::now()),
        );
    }
}

pub(super) fn manual_jobs() -> (crate::Session, ScriptLink, ProfileJobs, ScriptLink) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        testing::ProbeMovie::new()
            .combat_facts()
            .constant("flash.getGameObjectS", json!("\"player-a\""))
            .constant("flash.callGameFunction0", json!("true"))
            .constant("world.getAvailableMonsterMapIds", json!([2]))
            .constant("combat.getConsumableSkillItem", json!(null))
            .responding("combat.cancelAutoAttack", [(json!(null), vec![])])
            .responding("combat.cancelTarget", [(json!(null), vec![])])
            .recording("flash.getGameObject")
            .recording("combat.getTarget")
            .recording("combat.getSkillCooldownRemaining")
            .recording("world.getAvailableMonsterMapIds")
            .responding("combat.attackMonster", [(json!(true), vec![])])
            .responding("combat.useSkill", [(json!(true), vec![])]),
    );
    area(&observer, 1);
    let jobs = ProfileJobs::new(
        session.attach(ProfileJobs::INTERESTS).unwrap(),
        Waker::from(Arc::new(Unpark(std::thread::current()))),
    );
    let owner = session.attach(EventInterests::ALL).unwrap();
    (session, observer, jobs, owner)
}

#[test]
fn a_steady_attack_cast_cycle_uses_twelve_loader_callbacks() {
    let (_session, observer, mut jobs, owner) = manual_jobs();
    let id = start(&owner);
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    let calls = drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    assert_eq!(
        calls,
        [
            "flash.getGameObject",
            "flash.getGameObject",
            "flash.getGameObject",
            "flash.getGameObject",
            "flash.getGameObject",
            "combat.getTarget",
            "world.getAvailableMonsterMapIds",
            "world.getAvailableMonsterMapIds",
            "combat.attackMonster",
            "combat.getSkillCooldownRemaining",
            "world.getAvailableMonsterMapIds",
            "combat.useSkill"
        ]
    );
    owner.jobs.cancel(id);
}

#[test]
fn an_attack_completion_reuses_its_snapshot_and_a_trigger_observes_again() {
    let (_session, observer, mut jobs, owner) = manual_jobs();
    let id = start_profile(
        &owner,
        json!({"steps":[{"skill":1}], "messageTriggers":[{"messageIncludes":"enrage","skill":4}]}),
    );
    drive(&mut jobs, &observer, |jobs, _| {
        matches!(
            &jobs.jobs[0].waiting,
            Waiting::Action {
                command: Command {
                    action: Action::Attack(_),
                    ..
                },
                ..
            }
        )
    });
    drain(&observer);
    emit(&observer, json!({"cmd":"cb","p":{"player-a":{"intHP":25}}}));
    let calls = drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    assert!(
        !calls.iter().any(|name| name == "combat.getTarget"),
        "{calls:?}"
    );
    assert_eq!(jobs.jobs[0].snapshot.hp.current, 100.0);
    owner.jobs.engage(id, false);
    emit(
        &observer,
        json!({"cmd":"cb","anims":[{"msg":"enrage","animStr":"cast","cInf":"m:2","tInf":"p:7"}]}),
    );
    let calls = drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    assert!(
        calls.iter().any(|name| name == "combat.getTarget"),
        "{calls:?}"
    );
    assert_eq!(jobs.jobs[0].snapshot.hp.current, 25.0);
    owner.jobs.cancel(id);
}

#[test]
fn deferred_message_overflow_keeps_the_newest_messages_and_the_job_alive() {
    use tracing_subscriber::prelude::*;
    struct Warnings(Arc<std::sync::atomic::AtomicUsize>);
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Warnings {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if *event.metadata().level() == tracing::Level::WARN
                && event.metadata().target() == "lucent_host::game::jobs"
            {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    let warnings = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let subscriber = tracing_subscriber::registry().with(Warnings(warnings.clone()));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let (_session, observer, mut jobs, owner) = manual_jobs();
    let id = start_profile(
        &owner,
        json!({"steps":[{"skill":1}], "messageTriggers":[{"messageIncludes":"enrage","skill":4}]}),
    );
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.attackMonster")
    });
    for index in 0..18 {
        emit(
            &observer,
            json!({"cmd":"cb","anims":[{"msg":format!("enrage {index}"),"animStr":"cast","cInf":"m:2","tInf":"p:7"}]}),
        );
    }
    while let Some(event) = jobs.link.try_event().unwrap() {
        jobs.event(event, Instant::now());
    }
    assert!(!jobs.jobs[0].done);
    assert_eq!(
        jobs.jobs[0]
            .messages
            .iter()
            .map(|(_, message)| message.text.clone())
            .collect::<Vec<_>>(),
        (2..18)
            .map(|index| format!("enrage {index}"))
            .collect::<Vec<_>>()
    );
    let version = jobs.jobs[0].request.executor.target_version();
    jobs.jobs[0].feed(
        ProfileEvent::Message {
            observed_version: version,
            message: Message {
                text: "enrage deferred".into(),
                monster_map_id: Some(2),
                source: MessageSource::Animation,
            },
        },
        Instant::now(),
        jobs.clock,
        &jobs.link,
        &jobs.waker,
    );
    assert_eq!(jobs.jobs[0].messages.len(), 16);
    assert_eq!(jobs.jobs[0].messages.front().unwrap().1.text, "enrage 2");
    assert_eq!(warnings.load(Ordering::Relaxed), 1);
    owner.jobs.cancel(id);
}

#[test]
fn invalidation_abandons_pending_work_without_resetting_the_rotation() {
    let (_session, observer, mut jobs, owner) = manual_jobs();
    let id = start_profile(
        &owner,
        json!({"steps":[{"skill":1},{"skill":2}], "messageTriggers":[{"messageIncludes":"enrage","skill":4}]}),
    );
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    assert_eq!(next_skill(&owner), "1");
    drive(&mut jobs, &observer, |jobs, _| {
        jobs.jobs[0].request.executor.ready()
    });
    let version = jobs.jobs[0].request.executor.target_version();
    emit(
        &observer,
        json!({"cmd":"cb","anims":[{"msg":"enrage","animStr":"cast","cInf":"m:2","tInf":"p:7"}]}),
    );
    while let Some(event) = jobs.link.try_event().unwrap() {
        jobs.event(event, Instant::now());
    }
    assert_eq!(jobs.jobs[0].messages.len(), 1);
    jobs.event(ScriptEvent::Invalidated, Instant::now());
    assert!(!jobs.jobs[0].done);
    assert!(jobs.jobs[0].messages.is_empty());
    assert!(jobs.jobs[0].request.executor.target_version() > version);
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    assert_eq!(next_skill(&owner), "2");
    owner.jobs.cancel(id);
}

#[test]
fn connection_recovery_reacquires_the_ambient_claim_and_rebinds_cleanup() {
    let (_session, observer, mut jobs, owner) = manual_jobs();
    start_profile(&owner, json!({"steps":[{"skill":5}]}));
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    assert!(
        observer
            .acquire_consumable_claim(Some(101))
            .unwrap()
            .is_none()
    );
    observer
        .call_loader(
            "probe.emit",
            vec![
                Value::String("onConnection".into()),
                Value::String("OnConnection".into()),
            ],
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
    let released = observer
        .acquire_consumable_claim(Some(101))
        .unwrap()
        .unwrap();
    observer.release_claim(released.id);
    area(&observer, 1);
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    assert!(
        observer
            .acquire_consumable_claim(Some(101))
            .unwrap()
            .is_none()
    );
    drain(&observer);
    drop(owner);
    assert_eq!(
        drain(&observer),
        ["combat.cancelAutoAttack", "combat.cancelTarget"]
    );
    assert!(
        observer
            .acquire_consumable_claim(Some(101))
            .unwrap()
            .is_some()
    );
}

#[test]
fn repeated_invalidation_does_not_extend_the_observation_failure_bound() {
    let (_session, observer, mut jobs, owner) = manual_jobs();
    start(&owner);
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    let started = Instant::now();
    for elapsed in [0, 4900, 5000] {
        let now = started + Duration::from_millis(elapsed);
        jobs.event(ScriptEvent::Invalidated, now);
        assert!(!jobs.jobs[0].done);
        let (reply, pending) = Pending::channel();
        reply
            .send(Err(Error::Unavailable("combat facts".into())))
            .unwrap();
        jobs.jobs[0].waiting = Waiting::Observation {
            pending,
            event: ProfileEvent::Tick,
            deadline: now + Duration::from_secs(5),
        };
        jobs.jobs[0].turn(now, jobs.clock, &jobs.link, &jobs.waker);
        assert_eq!(jobs.jobs[0].done, elapsed == 5000);
    }
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    let mut failure = None;
    while let Some(event) = owner.try_event().unwrap() {
        if let ScriptEvent::JobFailed { error, .. } = event {
            failure = Some(error);
        }
    }
    assert_eq!(
        failure.as_deref(),
        Some(
            "combat profile observation failed for 5 seconds: the loader has not registered callback combat facts"
        )
    );
}

#[test]
fn controller_cleanup_leaves_the_parent_stop_for_run_teardown() {
    let (_session, observer, mut jobs, owner) = manual_jobs();
    start(&owner);
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    drain(&observer);
    jobs.jobs[0].request.detached.store(true, Ordering::Release);
    jobs.advance(Instant::now());
    drop(owner);
    assert_eq!(
        drain(&observer),
        ["combat.cancelAutoAttack", "combat.cancelTarget"]
    );
}
