mod common;

use common::{hold::hold, logs::Logs, *};
use lucent_host::bridge::{EventInterests, ScriptEvent};
use lucent_host::scripting::{Kill, RuntimeError, ScriptRun};
use lucent_host::testing;
use lucent_host::{ScriptLink, Session};
use lucent_protocol::bridge::Value;
use lucent_protocol::script::ScriptOutcome;
use serde_json::json;
use std::sync::{Arc, mpsc};
use std::task::{Wake, Waker};
use std::time::{Duration, Instant};
use tracing_subscriber::prelude::*;

const LOADING: &str = r#"
    import * as api from "lucent/api";
    console.log("module loading");
    export default function* () {
        yield* api.packet.sendToClient("started");
    }
"#;

const ACCEPTING: &str = r#"
    import * as api from "lucent/api";
    export default function* () {
        yield* api.drops.accept(70);
        yield* api.packet.sendToClient("started");
    }
"#;

const SKILL: &str = r#"
    import * as api from "lucent/api";
    import * as script from "lucent/script";
    console.log("module loading");
    export default function* () {
        yield* script.stop(String(yield* api.combat.useSkill(1)));
    }
"#;

const TARGETED_SKILL: &str = r#"
    import * as api from "lucent/api";
    import * as script from "lucent/script";
    export default function* () {
        yield* script.stop(String(yield* api.combat.useSkill(1, { target: 1 })));
    }
"#;

fn held(message: &str) -> (common::hold::Gate, tracing::dispatcher::DefaultGuard) {
    let (layer, gate) = hold(message, Some("lucent-js-"), true);
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(Logs::default())
            .with(layer),
    );
    (gate, tracing::dispatcher::set_default(&dispatch))
}

fn accepting_session() -> (lucent_host::Host, Session, ScriptLink) {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(
        &host,
        ready_movie().accepting_drop(
            Duration::from_secs(3600),
            json!({"type":"json","dataObj":{"cmd":"getDrop","ItemID":70,"bSuccess":1,"iQtyNow":1}})
                .to_string(),
        ),
    );
    log_in(&link);
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onExtensionResponse".into()),
            Value::String(
                json!({"type":"json","dataObj":{"cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Drop","iQty":1}}}})
                    .to_string(),
            ),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
    (host, session, link)
}

struct Changed(mpsc::Sender<()>);
impl Wake for Changed {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }
}

fn wait_last_end(session: &Session, expected: &str) {
    let (tx, changed) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Changed(tx))));
    let deadline = Instant::now() + TIMEOUT;
    while session.script_status().last_end.as_deref() != Some(expected) {
        if changed
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .is_err()
        {
            panic!("{:?}", session.script_status());
        }
    }
}

fn supervised_session() -> (lucent_host::Host, Session, ScriptLink) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    session.supervise(None).unwrap();
    log_in(&observer);
    (host, session, observer)
}

fn replacement_link(session: &mut Session) -> ScriptLink {
    let observer = session.attach(EventInterests::ALL).unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&observer);
    observer
}

fn accept_in_flight(session: &mut Session, observer: &ScriptLink) -> ScriptRun {
    let run = start(
        session,
        loose(&[("accept.js", ACCEPTING)]),
        Default::default(),
    );
    await_event(observer, "onProbeAccept", Some(&run));
    run
}

fn only_the_next_run_reaches_the_player(session: &mut Session, observer: &ScriptLink) {
    let marker = start(
        session,
        loose(&[(
            "marker.js",
            r#"import * as api from "lucent/api";
            export default function* () { yield* api.packet.sendToClient("marker"); }"#,
        )]),
        Default::default(),
    );
    assert_eq!(marker.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    let mut calls = 0;
    while let Some(event) = observer.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onDebug"
        {
            calls += 1;
        }
    }
    assert_eq!(calls, 1, "a dead run reached the player");
}

#[test]
fn a_kill_while_the_module_loads_issues_no_game_call() {
    let (gate, _guard) = held("module loading");
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[("load.js", LOADING)]),
        Default::default(),
    );
    gate.wait();
    run.kill();
    gate.open();
    let outcome = run.wait(TIMEOUT);
    assert!(
        matches!(outcome, Err(RuntimeError::Killed(Kill::Requested))),
        "{outcome:?}"
    );
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_player_restart_while_the_module_loads_issues_no_game_call() {
    let (gate, _guard) = held("module loading");
    let (_host, mut session, _observer) = supervised_session();
    session
        .start_script(loose(&[("load.js", LOADING)]))
        .unwrap();
    gate.wait();
    session.restart_player(testing::headless_surface()).unwrap();
    gate.open();
    wait_last_end(&session, "load.js ended because its player stopped");
    let observer = replacement_link(&mut session);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_watchdog_during_module_load_issues_no_game_call() {
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "spin.js",
            r#"
            import * as api from "lucent/api";
            while (true) {}
            export default function* () {
                yield* api.packet.sendToClient("started");
            }
        "#,
        )]),
        Default::default(),
    );
    let outcome = run.wait(TIMEOUT);
    assert!(
        matches!(outcome, Err(RuntimeError::Killed(Kill::Watchdog))),
        "{outcome:?}"
    );
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_stop_during_the_first_action_issues_no_later_game_call() {
    let (_host, mut session, observer) = accepting_session();
    let run = accept_in_flight(&mut session, &observer);
    run.stop_script();
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_kill_during_the_first_action_issues_no_later_game_call() {
    let (_host, mut session, observer) = accepting_session();
    let run = accept_in_flight(&mut session, &observer);
    run.kill();
    let outcome = run.wait(TIMEOUT);
    assert!(
        matches!(outcome, Err(RuntimeError::Killed(Kill::Requested))),
        "{outcome:?}"
    );
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_player_restart_during_the_first_action_issues_no_later_game_call() {
    let (_host, mut session, observer) = accepting_session();
    session.supervise(None).unwrap();
    session
        .start_script(loose(&[("accept.js", ACCEPTING)]))
        .unwrap();
    await_event(&observer, "onProbeAccept", None);
    session.restart_player(testing::headless_surface()).unwrap();
    wait_last_end(&session, "accept.js ended because its player stopped");
    drop(observer);
    let observer = replacement_link(&mut session);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_failing_event_handler_fails_the_run_with_its_error() {
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "handler.js",
            r#"
            import * as api from "lucent/api";
            import * as script from "lucent/script";
            export default function* () {
                yield* api.events.on({ type: "join-map" }, function* () {
                    throw new Error("handler broke");
                });
                yield* api.packet.sendToClient("started");
                yield* script.sleep("1 hour");
            }
        "#,
        )]),
        Default::default(),
    );
    await_event(&observer, "onDebug", Some(&run));
    observer
        .call_loader(
            "probe.emit",
            vec![
                Value::String("onExtensionResponse".into()),
                Value::String(
                    json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":2,"areaName":"battleon-2",
                        "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}})
                    .to_string(),
                ),
            ],
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
    match run.wait(TIMEOUT).unwrap() {
        ScriptOutcome::Failed { message, details } => {
            assert_eq!(message, "handler.js:6:27: handler broke");
            assert!(details.unwrap().contains("handler broke"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_projected_read_does_not_ask_the_loader() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        ready_movie()
            .constant("player.getHp", json!(999))
            .game_object("world.myAvatar.dataLeaf.intHP", json!(999)),
    );
    log_in(&link);
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onExtensionResponse".into()),
            Value::String(
                json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":1,"areaName":"buyhouse-1",
                    "uoBranch":[{"entID":7,"uoName":"player-a","intHP":37,"intHPMax":100}]}})
                .to_string(),
            ),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "hp.js",
            r#"
            import * as api from "lucent/api";
            import * as script from "lucent/script";
            export default function* () {
                yield* script.stop(String(yield* api.player.getHp()));
            }
        "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("37".into())
        }
    );
}

fn emit(link: &ScriptLink, packet: serde_json::Value) {
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onExtensionResponse".into()),
            Value::String(json!({"type":"json","dataObj":packet}).to_string()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}

fn boss_room() -> (lucent_host::Host, Session, ScriptLink) {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(
        &host,
        ready_movie()
            .constant(
                "combat.getTarget",
                json!({"type":"monster","monsterMapId":1,"monsterId":1,"name":"Boss","cell":"Enter",
                    "hp":100,"maxHp":100,"level":1,"race":"None","state":1}),
            )
            .constant("world.getAvailableMonsterMapIds", json!([1]))
            .constant("combat.getSkillCooldownRemaining", json!(0))
            .constant("combat.useSkill", json!(true))
            .recording("combat.cancelAutoAttack")
            .recording("combat.cancelTarget"),
    );
    log_in(&link);
    emit(
        &link,
        json!({"cmd":"moveToArea","areaId":2,"areaName":"battleon-1",
            "uoBranch":[{"entID":7,"uoName":"player-a","strFrame":"Enter","intHP":100,"intHPMax":100}],
            "mondef":[{"MonID":1,"strMonName":"Boss","sRace":"None","iLvl":1}],
            "monBranch":[{"MonID":1,"MonMapID":1,"intHP":100,"intHPMax":100,"intState":1}],
            "monmap":[{"MonMapID":1,"strFrame":"Enter"}]}),
    );
    emit(&link, json!({"cmd":"monsterDrops","MonMapID":1,"items":{}}));
    (host, session, link)
}

fn counter(link: &ScriptLink) {
    emit(
        link,
        json!({"cmd":"cb","anims":[{"msg":"Boss prepares a counter attack","animStr":"cast",
            "cInf":"p:7","tInf":"m:1"}]}),
    );
}

fn refused() -> ScriptOutcome {
    ScriptOutcome::StoppedByScript {
        reason: Some("false".into()),
    }
}

fn skill_after_a_held_counter(hold_at: &str, source: &str) -> ScriptOutcome {
    let (gate, _guard) = held(hold_at);
    let (_host, mut session, link) = boss_room();
    let run = start(
        &mut session,
        loose(&[("skill.js", source)]),
        Default::default(),
    );
    gate.wait();
    counter(&link);
    gate.open();
    run.wait(TIMEOUT).unwrap()
}

#[test]
fn a_counter_before_the_script_starts_refuses_its_first_skill() {
    assert_eq!(
        skill_after_a_held_counter("script worker booting", SKILL),
        refused()
    );
}

#[test]
fn a_counter_before_the_script_starts_refuses_its_first_skill_on_a_named_target() {
    assert_eq!(
        skill_after_a_held_counter("script worker booting", TARGETED_SKILL),
        refused()
    );
}

#[test]
fn a_counter_while_the_module_loads_refuses_its_first_skill() {
    assert_eq!(
        skill_after_a_held_counter("module loading", SKILL),
        refused()
    );
}

#[test]
fn a_counter_during_the_run_refuses_the_next_skill() {
    let (_host, mut session, link) = boss_room();
    let run = start(
        &mut session,
        loose(&[(
            "skill.js",
            r#"
            import * as api from "lucent/api";
            import * as script from "lucent/script";
            export default function* () {
                yield* api.events.once(
                    { type: "counter-attack-start" },
                    { trigger: api.packet.sendToClient("started") },
                );
                yield* script.stop(String(yield* api.combat.useSkill(1)));
            }
        "#,
        )]),
        Default::default(),
    );
    await_event(&link, "onDebug", Some(&run));
    counter(&link);
    assert_eq!(run.wait(TIMEOUT).unwrap(), refused());
}

#[test]
fn a_stop_while_the_module_loads_issues_no_game_call() {
    let (gate, _guard) = held("module loading");
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[("load.js", LOADING)]),
        Default::default(),
    );
    gate.wait();
    run.stop_script();
    gate.open();
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_supervised_stop_while_the_module_loads_issues_no_game_call() {
    let (gate, _guard) = held("module loading");
    let logs = Logs::current().unwrap();
    let (_host, mut session, observer) = supervised_session();
    session
        .start_script(loose(&[("load.js", LOADING)]))
        .unwrap();
    gate.wait();
    session.stop_script();
    logs.wait_for(tracing::Level::INFO, "session 1: stopping load.js");
    gate.open();
    wait_last_end(&session, "load.js was stopped");
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

fn debug_messages(observer: &ScriptLink) -> Vec<Vec<Value>> {
    let mut messages = Vec::new();
    while let Some(event) = observer.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onDebug"
        {
            messages.push(event.args);
        }
    }
    messages
}

fn queued_event(observer: &ScriptLink) {
    observer
        .call_loader(
            "probe.emit",
            vec![
                Value::String("onDebug".into()),
                Value::String("queued event".into()),
            ],
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
}

#[test]
fn a_stop_queued_behind_an_event_while_the_module_loads_issues_no_game_call() {
    let (gate, _guard) = held("module loading");
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[("load.js", LOADING)]),
        Default::default(),
    );
    gate.wait();
    queued_event(&observer);
    run.stop_script();
    gate.open();
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
    drop(run);
    assert_eq!(
        debug_messages(&observer),
        vec![vec![Value::String("queued event".into())]]
    );
}

#[test]
fn a_supervised_stop_queued_behind_an_event_while_the_module_loads_issues_no_game_call() {
    let (gate, _guard) = held("module loading");
    let logs = Logs::current().unwrap();
    let (_host, mut session, observer) = supervised_session();
    session
        .start_script(loose(&[("load.js", LOADING)]))
        .unwrap();
    gate.wait();
    queued_event(&observer);
    session.stop_script();
    logs.wait_for(tracing::Level::INFO, "session 1: stopping load.js");
    gate.open();
    wait_last_end(&session, "load.js was stopped");
    assert_eq!(
        debug_messages(&observer),
        vec![vec![Value::String("queued event".into())]]
    );
}

fn stopped_with(session: &mut Session, source: &str) -> String {
    let outcome = start(session, loose(&[("timers.js", source)]), Default::default())
        .wait(TIMEOUT)
        .unwrap();
    let ScriptOutcome::StoppedByScript {
        reason: Some(reason),
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    reason
}

#[test]
fn a_due_module_timer_runs_before_main() {
    let (_host, mut session, _runtime) = ready_runtime();
    let reason = stopped_with(
        &mut session,
        r#"
        import * as script from "lucent/script";
        let initialized = false;
        setTimeout(() => { initialized = true; }, 0);
        export default function* () {
            yield* script.stop(String(initialized));
        }
    "#,
    );
    assert_eq!(reason, "true");
}

const TIMER_ORDER: &str = r#"
    import { Effect } from "effect";
    import * as script from "lucent/script";
    const order = [];
    const top = new Promise((resolve) => setTimeout(() => {
        order.push("top-first");
        setTimeout(() => { order.push("top-nested"); resolve(); }, 0);
    }, 0));
    setTimeout(() => order.push("top-second"), 0);
    clearTimeout(setTimeout(() => order.push("cancelled"), 0));
    console.log("module loaded");
    export default function* () {
        order.push("main");
        yield* Effect.tryPromise(() => top);
        const after = new Promise((resolve) => setTimeout(() => {
            order.push("after-first");
            resolve();
        }, 0));
        yield* Effect.sleep(0);
        order.push("after-sleep");
        yield* Effect.tryPromise(() => after);
        yield* script.stop(JSON.stringify(order));
    }
"#;

fn assert_timer_order(reason: &str) {
    let order: Vec<String> = serde_json::from_str(reason).unwrap();
    assert_eq!(
        order,
        [
            "top-first",
            "top-second",
            "main",
            "top-nested",
            "after-first",
            "after-sleep"
        ]
    );
}

#[test]
fn module_timers_run_before_main_and_the_timers_they_schedule_after_it() {
    let (_host, mut session, _runtime) = ready_runtime();
    assert_timer_order(&stopped_with(&mut session, TIMER_ORDER));
}

#[test]
fn an_event_queued_while_the_module_loads_keeps_the_module_timer_order() {
    let (gate, _guard) = held("module loaded");
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[("timers.js", TIMER_ORDER)]),
        Default::default(),
    );
    gate.wait();
    queued_event(&observer);
    gate.open();
    let outcome = run.wait(TIMEOUT).unwrap();
    let ScriptOutcome::StoppedByScript {
        reason: Some(reason),
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    assert_timer_order(&reason);
}

#[test]
fn a_timer_set_in_main_fires_before_a_zero_sleep_while_events_arrive() {
    let (gate, _guard) = held("main waiting");
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "events.js",
            r#"
            import { Effect } from "effect";
            import * as script from "lucent/script";
            export default function* () {
                const order = [];
                const resumed = new Promise((resolve) => setTimeout(resolve, 0));
                console.log("main waiting");
                yield* Effect.tryPromise(() => resumed);
                const timer = new Promise((resolve) => setTimeout(() => {
                    order.push("timer");
                    resolve();
                }, 0));
                yield* Effect.sleep(0);
                order.push("sleep");
                yield* Effect.tryPromise(() => timer);
                yield* script.stop(JSON.stringify(order));
            }
        "#,
        )]),
        Default::default(),
    );
    gate.wait();
    queued_event(&observer);
    gate.open();
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some(r#"["timer","sleep"]"#.into())
        }
    );
}

const TIMER_BEFORE_MAIN: &str = r#"
    import * as api from "lucent/api";
    setTimeout(() => console.log("module timer"), 0);
    export default function* () {
        yield* api.packet.sendToClient("started");
    }
"#;

#[test]
fn a_stop_while_a_module_timer_runs_before_main_issues_no_game_call() {
    let (gate, _guard) = held("module timer");
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[("timer.js", TIMER_BEFORE_MAIN)]),
        Default::default(),
    );
    gate.wait();
    run.stop_script();
    gate.open();
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::Stopped { reason: None }
    );
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_kill_while_a_module_timer_runs_before_main_issues_no_game_call() {
    let (gate, _guard) = held("module timer");
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[("timer.js", TIMER_BEFORE_MAIN)]),
        Default::default(),
    );
    gate.wait();
    run.kill();
    gate.open();
    let outcome = run.wait(TIMEOUT);
    assert!(
        matches!(outcome, Err(RuntimeError::Killed(Kill::Requested))),
        "{outcome:?}"
    );
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_player_restart_while_a_module_timer_runs_before_main_issues_no_game_call() {
    let (gate, _guard) = held("module timer");
    let (_host, mut session, _observer) = supervised_session();
    session
        .start_script(loose(&[("timer.js", TIMER_BEFORE_MAIN)]))
        .unwrap();
    gate.wait();
    session.restart_player(testing::headless_surface()).unwrap();
    gate.open();
    wait_last_end(&session, "timer.js ended because its player stopped");
    let observer = replacement_link(&mut session);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}

#[test]
fn a_watchdog_in_a_module_timer_before_main_issues_no_game_call() {
    let (_host, mut session, _runtime) = ready_runtime();
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "spin.js",
            r#"
            import * as api from "lucent/api";
            setTimeout(() => { while (true) {} }, 0);
            export default function* () {
                yield* api.packet.sendToClient("started");
            }
        "#,
        )]),
        Default::default(),
    );
    let outcome = run.wait(TIMEOUT);
    assert!(
        matches!(outcome, Err(RuntimeError::Killed(Kill::Watchdog))),
        "{outcome:?}"
    );
    drop(run);
    only_the_next_run_reaches_the_player(&mut session, &observer);
}
