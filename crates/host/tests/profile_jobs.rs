mod common;

use common::*;
use lucent_host::{Supervisor, bridge::Value, testing};
use lucent_protocol::script::ScriptOutcome;
use serde_json::json;

fn packet(link: &lucent_host::ScriptLink, data: serde_json::Value) {
    let (event, payload) = if data.get("cmd").and_then(serde_json::Value::as_str) == Some("ct") {
        ("packetFromServer", json!({"t":"xt","b":{"o":data}}))
    } else {
        ("onExtensionResponse", json!({"type":"json","dataObj":data}))
    };
    link.call_loader(
        "probe.emit",
        vec![
            Value::String(event.into()),
            Value::String(payload.to_string()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}

fn setup() -> (lucent_host::Session, lucent_host::ScriptLink, Supervisor) {
    setup_with(|movie| movie)
}
fn setup_with(
    modify: impl FnOnce(testing::ProbeMovie) -> testing::ProbeMovie,
) -> (lucent_host::Session, lucent_host::ScriptLink, Supervisor) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        modify(
            ready_movie()
                .combat_facts()
                .constant("world.getAvailableMonsterMapIds", json!([2]))
                .constant("combat.getConsumableSkillItem", json!(null))
                .responding("combat.attackMonster", [(json!(true), vec![])])
                .responding("combat.useSkill", [(json!(true), vec![])])
                .responding("combat.cancelAutoAttack", [(json!(null), vec![])])
                .responding("combat.cancelTarget", [(json!(null), vec![])])
                .responding("drops.accept", [(json!(true), vec![])])
                .recording("settings.setWalkSpeed")
                .recording("settings.enemyMagnet"),
        ),
    );
    log_in(&observer);
    packet(
        &observer,
        json!({"cmd":"moveToArea","areaId":1,"areaName":"test-1",
        "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],
        "mondef":[{"MonID":1,"strMonName":"Frogzard"}],
        "monBranch":[{"MonID":1,"MonMapID":2,"intHP":50,"intHPMax":50}]}),
    );
    let supervisor = Supervisor::start(&mut session, None).unwrap();
    (session, observer, supervisor)
}

fn call(
    link: &lucent_host::ScriptLink,
    run: Option<&lucent_host::scripting::ScriptRun>,
    name: &str,
) -> Vec<Value> {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        assert!(std::time::Instant::now() < deadline, "missing {name}");
        let event = await_event(link, "onProbeCall", run);
        if event.args.first() == Some(&Value::String(name.into())) {
            return event.args;
        }
    }
}

#[test]
fn a_profile_kill_casts_from_host_cooldowns() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            const killed = yield* api.combat.kill("Frogzard", { profile: { steps: [{ skill: 1 }] } });
            yield* script.stop(String(killed));
        }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        call(&observer, Some(&run), "combat.attackMonster"),
        vec![
            Value::String("combat.attackMonster".into()),
            Value::List(vec![lucent_protocol::bridge::from_json(
                json!({"monsterMapId":2})
            )])
        ]
    );
    assert_eq!(
        call(&observer, Some(&run), "combat.useSkill"),
        vec![
            Value::String("combat.useSkill".into()),
            Value::List(vec![
                Value::String("1".into()),
                lucent_protocol::bridge::from_json(json!({"monsterMapId":2})),
                Value::Bool(false)
            ])
        ]
    );
    packet(
        &observer,
        json!({"cmd":"ct","m":{"2":{"intHP":0,"intState":0}}}),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
}

#[test]
fn a_killed_run_stops_combat_once() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () {
            yield* api.combat.kill("Frogzard", { profile: { steps: [{ skill: 1 }] } });
        }
    "#,
        )]),
        Default::default(),
    );
    call(&observer, Some(&run), "combat.attackMonster");
    run.kill();
    drop(run);
    call(&observer, None, "combat.cancelAutoAttack");
    call(&observer, None, "combat.cancelTarget");
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert!(
                !matches!(event.args.first(), Some(Value::String(name)) if name.starts_with("combat."))
            );
        }
    }
}

#[test]
fn a_killed_run_stops_a_parked_profile_once() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "farm.js",
            r#"
        import * as api from "lucent/api";
        export default function* () {
            yield* api.combat.killForItem("Frogzard", { item:"Scale", quantity:2 }, {
                profile: { steps:[{skill:1}] }
            });
        }
    "#,
        )]),
        Default::default(),
    );
    call(&observer, Some(&run), "combat.useSkill");
    packet(
        &observer,
        json!({"cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Scale","iQty":1,"sType":"Item"}}}),
    );
    packet(
        &observer,
        json!({"cmd":"ct","m":{"2":{"intHP":0,"intState":0}}}),
    );
    call(&observer, Some(&run), "drops.accept");
    drop(run);
    call(&observer, None, "combat.cancelAutoAttack");
    call(&observer, None, "combat.cancelTarget");
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert!(
                !matches!(event.args.first(), Some(Value::String(name)) if name.starts_with("combat."))
            );
        }
    }
}

#[test]
fn interrupting_a_kill_releases_its_claim_before_the_next_statement() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import { Effect } from "effect";
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* Effect.raceFirst(
                Effect.gen(function* () {
                    yield* Effect.raceFirst(
                        api.combat.kill("Frogzard", { profile: { steps: [{ skill: 5 }] } }),
                        api.packet.once({ command:"release", direction:"extension", encoding:"json" }, { timeout:"1 hour" }),
                    );
                    yield* api.packet.sendToClient("released");
                    yield* script.sleep("1 hour");
                }),
                Effect.gen(function* () {
                    yield* api.packet.once({ command:"sibling", direction:"extension", encoding:"json" }, { timeout:"1 hour" });
                    yield* api.packet.sendToClient("alive");
                    yield* script.sleep("1 hour");
                }),
            );
        }
    "#,
        )]),
        Default::default(),
    );
    call(&observer, Some(&run), "combat.attackMonster");
    packet(&observer, json!({"cmd":"release"}));
    call(&observer, Some(&run), "combat.cancelTarget");
    await_event(&observer, "onDebug", Some(&run));
    let claim = observer
        .acquire_consumable_claim(Some(777))
        .unwrap()
        .unwrap();
    observer.release_claim(claim.id);
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert!(
                !matches!(event.args.first(), Some(Value::String(name)) if name == "combat.attackMonster" || name == "combat.useSkill")
            );
        }
    }
    packet(&observer, json!({"cmd":"sibling"}));
    await_event(&observer, "onDebug", Some(&run));
    assert!(run.wait(std::time::Duration::ZERO).is_err());
}

#[test]
fn dropping_a_run_releases_claims_while_the_controller_is_busy() {
    use std::sync::{Arc, Mutex, mpsc};
    let (mut session, observer, _supervisor) = setup();
    let (entered, barrier) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let resume = Arc::new(Mutex::new(resume));
    session.set_profile_dispatch_probe(move |_, attack| {
        if attack {
            entered.send(()).unwrap();
            resume.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
        }
    });
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.combat.kill("Frogzard", { profile: { steps: [{ skill: 5 }] } }); }
    "#,
        )]),
        Default::default(),
    );
    barrier.recv_timeout(TIMEOUT).unwrap();
    drop(run);
    let claim = observer
        .acquire_consumable_claim(Some(777))
        .unwrap()
        .unwrap();
    observer.release_claim(claim.id);
    call(&observer, None, "combat.cancelTarget");
    release.send(()).unwrap();
    while session.live_profile_job_count() != 0 {
        let event = await_event(&observer, "onProbeCall", None);
        assert!(
            !matches!(event.args.first(), Some(Value::String(name)) if name == "combat.attackMonster" || name == "combat.useSkill")
        );
    }
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert!(
                !matches!(event.args.first(), Some(Value::String(name)) if name == "combat.attackMonster" || name == "combat.useSkill")
            );
        }
    }
}

#[test]
fn farm_kills_keep_the_cursor_and_triggers_while_accepting_drops() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "farm.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            const result = yield* api.combat.killForItem("Frogzard", { item:"Scale", quantity:2 }, {
                profile: { delayMs:50, steps:[{skill:1},{skill:2}], messageTriggers:[{messageIncludes:"enrage",skill:4}] }
            });
            yield* script.stop(String(result));
        }
    "#,
        )]),
        Default::default(),
    );
    let first = call(&observer, Some(&run), "combat.useSkill");
    assert!(matches!(&first[1], Value::List(args) if args[0] == Value::String("1".into())));
    packet(
        &observer,
        json!({"cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Scale","iQty":1,"sType":"Item"}}}),
    );
    packet(
        &observer,
        json!({"cmd":"ct","m":{"2":{"intHP":0,"intState":0}}}),
    );
    call(&observer, Some(&run), "drops.accept");
    packet(
        &observer,
        json!({"cmd":"ct","m":{"2":{"intHP":50,"intState":1}},"anims":[{"msg":"enrage","animStr":"cast","cInf":"m:2","tInf":"p:7"}]}),
    );
    loop {
        let event = await_event(&observer, "onProbeCall", Some(&run));
        assert_ne!(
            event.args.first(),
            Some(&Value::String("combat.attackMonster".into()))
        );
        if event.args.first() == Some(&Value::String("combat.useSkill".into())) {
            assert!(
                matches!(&event.args[1], Value::List(args) if args[0] == Value::String("4".into()) && args[2] == Value::Bool(true))
            );
            break;
        }
    }
    packet(
        &observer,
        json!({"cmd":"getDrop","ItemID":70,"bSuccess":1,"iQtyNow":1}),
    );
    let second = call(&observer, Some(&run), "combat.useSkill");
    assert!(matches!(&second[1], Value::List(args) if args[0] == Value::String("2".into())));
    packet(
        &observer,
        json!({"cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Scale","iQty":1,"sType":"Item"}}}),
    );
    packet(
        &observer,
        json!({"cmd":"ct","m":{"2":{"intHP":0,"intState":0}}}),
    );
    call(&observer, Some(&run), "drops.accept");
    packet(
        &observer,
        json!({"cmd":"getDrop","ItemID":70,"bSuccess":1,"iQtyNow":2}),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
}

#[test]
fn an_in_flight_action_completes_before_its_deferred_trigger() {
    let (mut session, observer, _supervisor) = setup_with(|movie| {
        movie.responding("combat.attackMonster", [(json!(true), vec![("packetFromServer".into(), json!({"t":"xt","b":{"o":{"cmd":"ct","anims":[{"msg":"enrage","animStr":"cast","cInf":"m:2","tInf":"p:7"}]}}}).to_string())])])
    });
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.combat.kill("Frogzard", { profile: {
            delayMs:60000, steps:[{skill:1}], messageTriggers:[{messageIncludes:"enrage",skill:4}]
        } }); }
    "#,
        )]),
        Default::default(),
    );
    let ordinary = call(&observer, Some(&run), "combat.useSkill");
    let trigger = call(&observer, Some(&run), "combat.useSkill");
    assert!(
        matches!(&ordinary[1], Value::List(args) if args[0] == Value::String("1".into()) && args[2] == Value::Bool(false))
    );
    assert!(
        matches!(&trigger[1], Value::List(args) if args[0] == Value::String("4".into()) && args[2] == Value::Bool(true))
    );
}

#[test]
fn persistent_observation_failure_defects_the_waiting_kill() {
    let (mut session, _observer, _supervisor) =
        setup_with(|movie| movie.game_object("world.actions.active", json!({})));
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.combat.kill("Frogzard", { profile: { steps:[{skill:1}] } }); }
    "#,
        )]),
        Default::default(),
    );
    assert!(
        matches!(run.wait(TIMEOUT).unwrap(), ScriptOutcome::Failed { message, .. } if message.contains("observation failed for 5 seconds"))
    );
}

#[test]
fn a_live_profile_keeps_the_native_settings_cadence() {
    use std::time::{Duration, Instant};
    let (mut session, observer, _supervisor) = setup();
    observer
        .command(
            serde_json::from_value(
                json!({"kind":"updateSettings","patch":{"enemyMagnetEnabled":true}}),
            )
            .unwrap(),
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.combat.kill("Frogzard", { profile: { delayMs:50, steps:[{skill:1}] } }); }
    "#,
        )]),
        Default::default(),
    );
    call(&observer, Some(&run), "combat.useSkill");
    let mut times = [Vec::new(), Vec::new()];
    let mut casts = 0;
    while times[0].len() < 6 {
        let event = await_event(&observer, "onProbeCall", Some(&run));
        match event.args.first() {
            Some(Value::String(name)) if name == "settings.setWalkSpeed" => {
                times[0].push(Instant::now())
            }
            Some(Value::String(name)) if name == "settings.enemyMagnet" => {
                times[1].push(Instant::now())
            }
            Some(Value::String(name)) if name == "combat.useSkill" => casts += 1,
            _ => {}
        }
    }
    assert!(casts >= 20);
    for (times, bound) in times
        .into_iter()
        .zip([Duration::from_millis(1500), Duration::from_secs(1)])
    {
        assert!(times.len() >= 6);
        assert!(times.windows(2).all(|pair| pair[1] - pair[0] <= bound));
    }
    assert!(run.wait(Duration::ZERO).is_err());
}

#[test]
fn a_loader_cast_exception_advances_the_rotation() {
    let (mut session, observer, _supervisor) = setup_with(|movie| {
        movie
            .throwing("combat.useSkill", "cast rejected")
            .recording("combat.useSkill")
    });
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.combat.kill("Frogzard", {profile:{steps:[{skill:1},{skill:2}]}}); }
    "#,
        )]),
        Default::default(),
    );
    for skill in ["1", "2"] {
        let cast = call(&observer, Some(&run), "combat.useSkill");
        assert!(matches!(&cast[1], Value::List(args) if args[0] == Value::String(skill.into())));
    }
    assert!(run.wait(std::time::Duration::ZERO).is_err());
}

#[test]
fn a_loader_attack_exception_retries_without_casting() {
    let (mut session, observer, _supervisor) = setup_with(|movie| {
        movie
            .throwing("combat.attackMonster", "attack rejected")
            .recording("combat.attackMonster")
    });
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.combat.kill("Frogzard", {profile:{steps:[{skill:1}]}}); }
    "#,
        )]),
        Default::default(),
    );
    call(&observer, Some(&run), "combat.attackMonster");
    loop {
        let event = await_event(&observer, "onProbeCall", Some(&run));
        assert_ne!(
            event.args.first(),
            Some(&Value::String("combat.useSkill".into()))
        );
        if event.args.first() == Some(&Value::String("combat.attackMonster".into())) {
            break;
        }
    }
    assert!(run.wait(std::time::Duration::ZERO).is_err());
}

#[test]
fn a_missing_global_timestamp_keeps_the_native_profile_ready() {
    let (mut session, observer, _supervisor) =
        setup_with(|movie| movie.without_game_object("world.GCDTS"));
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.combat.kill("Frogzard", {profile:{steps:[{skill:1}]}}); }
    "#,
        )]),
        Default::default(),
    );
    let cast = call(&observer, Some(&run), "combat.useSkill");
    assert!(matches!(&cast[1], Value::List(args) if args[0] == Value::String("1".into())));
}

#[test]
fn a_reconnected_profile_kill_keeps_fighting_until_death() {
    let (mut session, observer, _supervisor) = setup();
    let run = start(
        &mut session,
        loose(&[(
            "fight.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            const killed = yield* api.combat.kill("Frogzard", {profile:{delayMs:50,steps:[{skill:1}]}});
            yield* script.stop(String(killed));
        }
    "#,
        )]),
        Default::default(),
    );
    call(&observer, Some(&run), "combat.useSkill");
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
    log_in(&observer);
    packet(
        &observer,
        json!({"cmd":"moveToArea","areaId":1,"areaName":"test-1",
        "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],
        "mondef":[{"MonID":1,"strMonName":"Frogzard"}],
        "monBranch":[{"MonID":1,"MonMapID":2,"intHP":50,"intHPMax":50,"intState":1}]}),
    );
    call(&observer, Some(&run), "combat.useSkill");
    packet(
        &observer,
        json!({"cmd":"ct","m":{"2":{"intHP":0,"intState":0}}}),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
}
