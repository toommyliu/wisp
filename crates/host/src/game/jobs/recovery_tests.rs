use super::tests::{emit, manual_jobs, start_profile};
use super::*;
use crate::bridge::{ScriptEvent, Value};
use serde_json::json;
use std::sync::mpsc;
const TIMEOUT: Duration = Duration::from_secs(10);

fn calls(link: &ScriptLink) -> Vec<String> {
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

fn turn_until(jobs: &mut ProfileJobs, at: Instant, until: impl Fn(&Job) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        assert!(
            Instant::now() < deadline,
            "turn never reached the expected state"
        );
        let clock = jobs.clock;
        let link = &jobs.link;
        let waker = jobs.waker.clone();
        jobs.jobs[0].turn(at.max(Instant::now()), clock, link, &waker);
        if until(&jobs.jobs[0]) {
            return;
        }
        std::thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
    }
}

fn block_player(session: &crate::Session) -> (mpsc::Sender<()>, crate::bridge::Pending<()>) {
    let (entered, barrier) = mpsc::channel();
    let (release, resume) = mpsc::channel::<()>();
    let blocked = session.read_projection(move |_| {
        entered.send(()).unwrap();
        resume.recv_timeout(TIMEOUT).unwrap();
    });
    barrier.recv_timeout(TIMEOUT).unwrap();
    (release, blocked)
}

#[test]
fn event_overflow_fences_a_queued_forced_cast() {
    for recover in [true, false] {
        let (session, observer, mut jobs, owner) = manual_jobs();
        let stale = Arc::new(Mutex::new(Vec::new()));
        {
            let stale = stale.clone();
            session.set_profile_action_probe(move |value| stale.lock().push(value));
        }
        let id = start_profile(
            &owner,
            json!({"steps":[{"skill":1}],"messageTriggers":[{"messageIncludes":"enrage","skill":4}]}),
        );
        drive(&mut jobs, &observer, |_, calls| {
            calls.iter().any(|name| name == "combat.useSkill")
        });
        drive(&mut jobs, &observer, |jobs, _| {
            jobs.jobs[0].request.executor.ready() && matches!(jobs.jobs[0].waiting, Waiting::None)
        });
        owner.jobs.engage(id, false);
        jobs.advance(Instant::now());
        calls(&observer);
        if recover {
            for index in 0..1100 {
                emit(
                    &observer,
                    json!({"cmd":"cb","anims":[{"msg":format!("noise {index}"),"animStr":"cast","cInf":"m:2","tInf":"p:7"}]}),
                );
                if index % 100 == 99 {
                    calls(&observer);
                }
            }
            assert!(jobs.link.overflowed(), "the job link did not overflow");
        }
        stale.lock().clear();
        let version = jobs.jobs[0].request.executor.target_version();
        jobs.jobs[0].queue_message(
            (
                version,
                Message {
                    text: "enrage".into(),
                    monster_map_id: None,
                    source: MessageSource::Animation,
                },
            ),
            false,
        );
        let now = Instant::now();
        let saw = std::cell::Cell::new(false);
        turn_until(&mut jobs, now, |job| {
            if matches!(job.waiting, Waiting::Observation { .. }) {
                saw.set(true);
            }
            saw.get() && matches!(job.waiting, Waiting::None)
        });
        let later = now + Duration::from_millis(300);
        let scheduling_deadline = Instant::now() + TIMEOUT;
        let (release, blocked) = loop {
            assert!(
                Instant::now() < scheduling_deadline,
                "forced cast did not reach its observation"
            );
            let clock = jobs.clock;
            let waker = jobs.waker.clone();
            jobs.jobs[0].turn(later.max(Instant::now()), clock, &jobs.link, &waker);
            if matches!(jobs.jobs[0].waiting, Waiting::Observation { .. }) {
                break block_player(&session);
            }
            std::thread::yield_now();
        };
        turn_until(&mut jobs, later, |job| {
            matches!(job.waiting, Waiting::Action { cycle: None, .. })
        });
        let recovery = jobs.jobs[0].request.gate.state.lock().recovery;
        if recover {
            jobs.advance(later.max(Instant::now()));
            assert_eq!(
                jobs.jobs[0].request.gate.state.lock().recovery,
                recovery + 1,
                "overflow did not recover the job"
            );
            assert!(!jobs.jobs[0].done);
        }
        release.send(()).unwrap();
        blocked.recv_timeout(TIMEOUT).unwrap();
        let deadline = Instant::now() + TIMEOUT;
        while stale.lock().is_empty() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let seen = calls(&observer);

        if recover {
            assert_eq!(stale.lock().first(), Some(&true), "{seen:?}");
            assert!(
                !seen.iter().any(|name| name == "combat.useSkill"),
                "the pre-recovery forced cast reached the loader: {seen:?}"
            );
        } else {
            assert_eq!(stale.lock().first(), Some(&false));
            assert!(
                seen.iter().any(|name| name == "combat.useSkill"),
                "{seen:?}"
            );
        }
        owner.jobs.cancel(id);
    }
}

#[test]
fn event_overflow_keeps_the_job_fighting_with_its_claim() {
    let (_session, observer, mut jobs, owner) = manual_jobs();
    let id = start_profile(&owner, json!({"steps":[{"skill":5}]}));
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    for index in 0..1100 {
        emit(
            &observer,
            json!({"cmd":"cb","anims":[{"msg":format!("noise {index}"),"animStr":"cast","cInf":"m:2","tInf":"p:7"}]}),
        );
        if index % 100 == 99 {
            calls(&observer);
        }
    }
    assert!(jobs.link.overflowed());
    let calls = drive(&mut jobs, &observer, |_, calls| {
        calls
            .iter()
            .filter(|name| *name == "combat.useSkill")
            .count()
            >= 3
    });
    assert!(!calls.iter().any(|name| name.starts_with("combat.cancel")));
    assert!(
        observer
            .acquire_consumable_claim(Some(999))
            .unwrap()
            .is_none()
    );
    assert_eq!(jobs.jobs.len(), 1);
    owner.jobs.cancel(id);
}

#[test]
fn a_missing_player_idles_a_script_job_past_the_failure_bound() {
    let (_session, observer, mut jobs, owner) = manual_jobs();
    let id = start_profile(&owner, json!({"steps":[{"skill":1}]}));
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
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
    emit(
        &observer,
        json!({"cmd":"moveToArea","areaId":2,"areaName":"empty","uoBranch":[],"monBranch":[]}),
    );
    assert!(
        observer
            .read(|state| state.world.get_me().is_none())
            .unwrap()
    );
    let start = Instant::now();
    for elapsed in (0..=6500).step_by(250) {
        let now = start + Duration::from_millis(elapsed);
        for _ in 0..4 {
            jobs.advance(now);
            observer
                .call_loader("probe.echo", vec![])
                .recv_timeout(TIMEOUT)
                .unwrap();
        }
        assert_eq!(
            jobs.jobs.len(),
            1,
            "job failed after {elapsed} ms without a player"
        );
        assert!(
            jobs.jobs[0].observation_failure.is_none(),
            "no player is not an observation failure"
        );
    }
    emit(
        &observer,
        json!({"cmd":"moveToArea","areaId":3,"areaName":"test-3","uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],"mondef":[{"MonID":1,"strMonName":"Boss"}],"monBranch":[{"MonID":1,"MonMapID":2,"intHP":50,"intHPMax":50,"intState":1}]}),
    );
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    owner.jobs.cancel(id);
}

fn enable_auto(jobs: &mut ProfileJobs, observer: &ScriptLink, profile: serde_json::Value) {
    let pending = observer.command(
        serde_json::from_value(
            json!({"kind":"enableAutoAttack","profile":profile,"targetPriority":[]}),
        )
        .unwrap(),
    );
    jobs.advance(Instant::now());
    let record: serde_json::Value =
        serde_json::from_str(&pending.recv_timeout(TIMEOUT).unwrap()).unwrap();
    assert_eq!(record["config"]["enabled"], true);
    assert_eq!(record["status"]["running"], true);
}

#[test]
fn auto_attack_has_a_slot_outside_the_script_limit() {
    let (_session, observer, mut jobs, owner) = manual_jobs();
    for _ in 0..32 {
        owner
            .jobs
            .start(
                &json!({"profile":{"steps":[{"skill":1}]},"targetPriority":["Boss"]}).to_string(),
            )
            .unwrap();
    }
    assert_eq!(
        owner
            .jobs
            .start(&json!({"profile":{},"targetPriority":[]}).to_string())
            .unwrap_err(),
        "too many combat profile jobs"
    );
    enable_auto(&mut jobs, &observer, json!({"steps":[{"skill":1}]}));
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    assert_eq!(jobs.requests.handoff.lock().live, 33);
    assert_eq!(
        owner
            .jobs
            .start(&json!({"profile":{},"targetPriority":[]}).to_string())
            .unwrap_err(),
        "too many combat profile jobs"
    );
}

#[test]
fn event_overflow_keeps_session_auto_attack_fighting() {
    let (_session, observer, mut jobs, _owner) = manual_jobs();
    enable_auto(&mut jobs, &observer, json!({"steps":[{"skill":5}]}));
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    for index in 0..1100 {
        emit(
            &observer,
            json!({"cmd":"cb","anims":[{"msg":format!("noise {index}"),"animStr":"cast","cInf":"m:2","tInf":"p:7"}]}),
        );
        if index % 100 == 99 {
            calls(&observer);
        }
    }
    assert!(jobs.link.overflowed());
    let calls = drive(&mut jobs, &observer, |_, calls| {
        calls
            .iter()
            .filter(|name| *name == "combat.useSkill")
            .count()
            >= 3
    });
    assert!(!calls.iter().any(|name| name.starts_with("combat.cancel")));
    assert!(
        observer
            .acquire_consumable_claim(Some(999))
            .unwrap()
            .is_none()
    );
    assert_eq!(jobs.jobs.len(), 1);
    let record: serde_json::Value = serde_json::from_str(
        &observer
            .query(serde_json::from_value(json!({"kind":"autoAttack"})).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(record["config"]["enabled"], true);
    assert!(record["status"]["lastError"].is_null());
}

#[test]
fn rebuilding_session_jobs_stops_only_the_old_job_once() {
    let (mut session, observer, mut jobs, _owner) = manual_jobs();
    enable_auto(&mut jobs, &observer, json!({"steps":[{"skill":1}]}));
    drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    calls(&observer);
    let waker = jobs.waker.clone();
    drop(jobs);
    let stopped = calls(&observer);
    assert_eq!(
        stopped
            .iter()
            .filter(|name| *name == "combat.cancelAutoAttack")
            .count(),
        1
    );
    assert_eq!(
        stopped
            .iter()
            .filter(|name| *name == "combat.cancelTarget")
            .count(),
        1
    );
    let mut jobs = ProfileJobs::new(session.attach(ProfileJobs::INTERESTS).unwrap(), waker);
    let resumed = drive(&mut jobs, &observer, |_, calls| {
        calls.iter().any(|name| name == "combat.useSkill")
    });
    assert!(!resumed.iter().any(|name| name.starts_with("combat.cancel")));
    assert!(session.auto_attack().config.enabled);
}

#[test]
fn session_jobs_idle_before_the_first_viewport() {
    for profile in [
        json!({"steps":[{"skill":1}]}),
        json!({"consumable":"Potion","steps":[{"skill":5}]}),
        json!({"classNames":["Mage"],"steps":[{"skill":1}]}),
    ] {
        let host = crate::testing::headless_host();
        let mut session = host
            .start_session(crate::testing::headless_surface())
            .unwrap();
        let observer = session.attach(crate::bridge::EventInterests::ALL).unwrap();
        let mut jobs = ProfileJobs::new(
            session.attach(ProfileJobs::INTERESTS).unwrap(),
            Waker::noop().clone(),
        );
        enable_auto(&mut jobs, &observer, profile);
        let start = Instant::now();
        for elapsed in (0..=6500).step_by(250) {
            let now = start + Duration::from_millis(elapsed);
            for _ in 0..4 {
                jobs.advance(now);
                assert_eq!(
                    session.attached_link_count().recv_timeout(TIMEOUT).unwrap(),
                    2
                );
            }
            let record = session.auto_attack();
            assert!(
                record.config.enabled,
                "before a viewport after {elapsed} ms: {record:?}"
            );
            assert!(record.status.last_error.is_none());
            assert_eq!(jobs.jobs.len(), 1);
            assert!(jobs.jobs[0].observation_failure.is_none());
        }
    }
}
