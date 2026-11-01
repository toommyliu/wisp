mod common;

use common::{logs::Logs, *};
use lucent_host::bridge::EventInterests;
use lucent_host::{ScriptRequest, Supervisor};
use lucent_host::{ViewEvent, Viewport, testing};
const INFO: tracing::Level = tracing::Level::INFO;
const WARN: tracing::Level = tracing::Level::WARN;

fn session_lines(logs: &Logs, level: tracing::Level) -> Vec<String> {
    logs.messages(level)
        .into_iter()
        .filter(|s| s.starts_with("session "))
        .collect()
}

const WAIT: &str = r#"
    import * as api from "lucent/api";
    import * as script from "lucent/script";
    export default function* () {
        yield* api.packet.sendToClient("started");
        yield* script.sleep("1 hour");
    }
"#;

#[test]
fn the_daily_script_runs_after_login() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    let script = lucent_host::library::load_script(
        "@lucent/dailies/scripts/farm-elders-blood.js",
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    )
    .unwrap();
    let (request, _) = ScriptRequest::new(script);
    let _supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
    logs.wait_for(
        INFO,
        "session 1: @lucent/dailies/scripts/farm-elders-blood.js is waiting for login",
    );
    assert_eq!(session_lines(&logs, INFO).len(), 2);
    log_in(&observer);
    logs.wait_for(tracing::Level::DEBUG, "session 1: script worker reaped");
    assert_eq!(
        session_lines(&logs, INFO),
        [
            "session 1: supervisor started",
            "session 1: @lucent/dailies/scripts/farm-elders-blood.js is waiting for login",
            "session 1: @lucent/dailies/scripts/farm-elders-blood.js started",
            "session 1: @lucent/dailies/scripts/farm-elders-blood.js completed",
        ]
    );
    assert_eq!(session_lines(&logs, WARN), Vec::<String>::new());
}

#[test]
fn stop_and_kill_reach_the_run_from_the_controls() {
    for kill in [false, true] {
        let logs = Logs::default();
        let dispatch = logs.dispatch();
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let (mut session, observer) = testing::start_probe(&host, ready_movie());
        let (request, controls) = ScriptRequest::new(loose(&[("wait.js", WAIT)]));
        let _supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
        logs.wait_for(INFO, "session 1: wait.js is waiting for login");
        log_in(&observer);
        assert_eq!(
            await_event(&observer, "onDebug", None).args,
            vec![lucent_protocol::bridge::Value::String("started".into())]
        );
        if kill {
            controls.kill();
            logs.wait_for(WARN, "session 1: wait.js was killed");
            assert!(session_lines(&logs, INFO).contains(&"session 1: killing wait.js".into()));
        } else {
            controls.stop();
            logs.wait_for(INFO, "session 1: wait.js was stopped");
            assert!(session_lines(&logs, INFO).contains(&"session 1: stopping wait.js".into()));
        }
        logs.wait_for(tracing::Level::DEBUG, "session 1: script worker reaped");
    }
}

#[test]
fn a_restart_ends_the_run_and_does_not_rerun() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    let (request, controls) = ScriptRequest::new(loose(&[("wait.js", WAIT)]));
    let mut supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
    logs.wait_for(INFO, "session 1: wait.js is waiting for login");
    log_in(&observer);
    await_event(&observer, "onDebug", None);
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    logs.wait_for_count(INFO, "session 1: supervisor started", 2);
    let observer = session.attach(EventInterests::ALL).unwrap();
    session.send(ViewEvent::Resized(
        Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&observer);
    log_in(&observer);
    controls.status();
    logs.wait_for(INFO, "session 1: no script is running");
    assert_eq!(
        session_lines(&logs, WARN),
        ["session 1: wait.js ended because its player stopped"]
    );
    let info = session_lines(&logs, INFO);
    assert_eq!(
        info.iter()
            .filter(|s| s.as_str() == "session 1: wait.js started")
            .count(),
        1
    );
    assert_eq!(&info[info.len() - 1..], ["session 1: no script is running"]);
}

#[test]
fn a_player_restart_cancels_a_script_waiting_for_login() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, _) = testing::start_probe(&host, ready_movie());
    let (request, controls) =
        ScriptRequest::new(loose(&[("done.js", "export default function* () {}")]));
    let mut supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
    logs.wait_for(INFO, "session 1: done.js is waiting for login");
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    logs.wait_for_count(INFO, "session 1: supervisor started", 2);
    let observer = session.attach(EventInterests::ALL).unwrap();
    session.send(ViewEvent::Resized(
        Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&observer);
    log_in(&observer);
    controls.status();
    logs.wait_for(INFO, "session 1: no script is running");
    assert!(!session_lines(&logs, INFO).contains(&"session 1: done.js started".into()));
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("done.js ended because its player stopped")
    );
}

#[test]
fn controls_cancel_a_waiting_script_without_replaying_it_after_login() {
    for kill in [false, true] {
        let logs = Logs::default();
        let dispatch = logs.dispatch();
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let (mut session, observer) = testing::start_probe(&host, ready_movie());
        let (request, controls) =
            ScriptRequest::new(loose(&[("done.js", "export default function* () {}")]));
        if kill {
            controls.kill();
        } else {
            controls.stop();
        }
        let _supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
        let (level, end) = if kill {
            (WARN, "session 1: done.js was killed")
        } else {
            (INFO, "session 1: done.js was stopped")
        };
        logs.wait_for(level, end);
        log_in(&observer);
        controls.status();
        logs.wait_for(INFO, "session 1: no script is running");
        assert!(!session_lines(&logs, INFO).contains(&"session 1: done.js started".into()));
    }
}

#[test]
fn an_already_logged_in_player_runs_without_another_login_event() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    log_in(&observer);
    let (request, _) = ScriptRequest::new(loose(&[("done.js", "export default function* () {}")]));
    let _supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
    logs.wait_for(tracing::Level::DEBUG, "session 1: script worker reaped");
    assert_eq!(
        session_lines(&logs, INFO),
        [
            "session 1: supervisor started",
            "session 1: done.js is waiting for login",
            "session 1: done.js started",
            "session 1: done.js completed",
        ]
    );
}

#[test]
fn a_skill_five_profile_queued_before_connection_completes_after_login() {
    use lucent_protocol::bridge::Value;
    use serde_json::json;

    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        ready_movie()
            .combat_facts()
            .constant("world.isActionAvailable", json!(true))
            .constant("world.getAvailableMonsterMapIds", json!([2]))
            .constant("combat.getConsumableSkillItem", json!(null))
            .responding(
                "combat.attackMonster",
                [(
                    json!(true),
                    vec![(
                        "packetFromServer".into(),
                        json!({"t":"xt","b":{"o":{"cmd":"ct","m":{"2":{"intHP":0,"intState":0}}}}})
                            .to_string(),
                    )],
                )],
            ),
    );
    let (request, _) = ScriptRequest::new(loose(&[(
        "fight.js",
        r#"
        import * as api from "lucent/api";
        export default function* () {
            const killed = yield* api.combat.kill("Frogzard", { profile: { steps: [{ skill: 5 }] } });
            console.log("profile killed " + killed);
        }
    "#,
    )]));
    let _supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
    logs.wait_for(INFO, "session 1: fight.js is waiting for login");
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
    for data in [
        json!({"cmd":"moveToArea","areaId":1,"areaName":"test-1",
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],
            "mondef":[{"MonID":1,"strMonName":"Frogzard"}],
            "monBranch":[{"MonID":1,"MonMapID":2,"intHP":50,"intHPMax":50}]}),
        json!({"cmd":"loadInventoryBig","items":[],"hitems":[{"ItemID":2,"sName":"Home","sType":"House","bEquip":1}]}),
    ] {
        observer
            .call_loader(
                "probe.emit",
                vec![
                    Value::String("onExtensionResponse".into()),
                    Value::String(json!({"type":"json","dataObj":data}).to_string()),
                ],
            )
            .recv_timeout(TIMEOUT)
            .unwrap();
    }
    logs.wait_for(tracing::Level::DEBUG, "session 1: script worker reaped");
    assert_eq!(session_lines(&logs, WARN), Vec::<String>::new());
    assert!(logs.messages(INFO).contains(&"profile killed true".into()));
    assert!(session_lines(&logs, INFO).contains(&"session 1: fight.js completed".into()));
}

#[test]
fn a_concurrent_start_is_busy_and_leaves_the_active_run_alive() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    let (request, controls) = ScriptRequest::new(loose(&[("a.js", WAIT)]));
    let mut supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
    log_in(&observer);
    await_event(&observer, "onDebug", None);
    let result = supervisor.start_script(&mut session, loose(&[("b.js", WAIT)]));
    assert_eq!(
        result.err().map(|error| error.to_string()).as_deref(),
        Some("a script is already active")
    );
    controls.status();
    logs.wait_for(INFO, "session 1: a.js is running");
    controls.kill();
    logs.wait_for(WARN, "session 1: a.js was killed");
}

#[test]
fn controls_from_a_finished_run_cannot_kill_its_successor() {
    let logs = Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    let (request, old) = ScriptRequest::new(loose(&[("a.js", "export default function* () {}")]));
    let mut supervisor = Supervisor::start(&mut session, Some(request)).unwrap();
    log_in(&observer);
    logs.wait_for(tracing::Level::DEBUG, "session 1: script worker reaped");
    let current = supervisor
        .start_script(&mut session, loose(&[("b.js", WAIT)]))
        .unwrap();
    await_event(&observer, "onDebug", None);
    old.kill();
    current.status();
    logs.wait_for(INFO, "session 1: b.js is running");
    assert_eq!(
        session.script_status().phase,
        lucent_protocol::script::ScriptPhase::Running
    );
    assert!(session_lines(&logs, WARN).is_empty());
    current.kill();
    logs.wait_for(WARN, "session 1: b.js was killed");
}
