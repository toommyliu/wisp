#![cfg(any(target_os = "macos", target_os = "linux"))]

mod common;

use common::*;
use lucent_host::Supervisor;
use lucent_host::{bridge::Value, testing};

#[test]
fn only_an_active_script_owns_a_js_worker_and_watchdog() {
    let logs = logs::Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        ready_movie()
            .combat_facts()
            .recording("settings.setWalkSpeed")
            .constant("world.getAvailableMonsterMapIds", serde_json::json!([2]))
            .responding("combat.attackMonster", [(serde_json::json!(true), vec![])])
            .constant("combat.useSkill", serde_json::json!(true)),
    );
    log_in(&observer);
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    for _ in 0..2 {
        while await_event(&observer, "onProbeCall", None).args.first()
            != Some(&Value::String("settings.setWalkSpeed".into()))
        {}
    }
    assert_eq!(js_threads(), Vec::<String>::new());
    let controls = supervisor
        .start_script(
            &mut session,
            loose(&[(
                "wait.js",
                r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* api.packet.sendToClient("started");
            yield* script.sleep("1 hour");
        }
    "#,
            )]),
        )
        .unwrap();
    await_event(&observer, "onDebug", None);
    #[cfg(target_os = "macos")]
    let expected = vec!["lucent-js-1", "lucent-js-watchdog-1"];
    #[cfg(target_os = "linux")]
    let expected = vec!["lucent-js-1", "lucent-js-watch"];
    assert_eq!(js_threads(), expected);
    controls.stop();
    logs.wait_for(tracing::Level::DEBUG, "session 1: script worker reaped");
    assert_eq!(js_threads(), Vec::<String>::new());

    let fresh_links = session.attached_link_count().recv_timeout(TIMEOUT).unwrap();
    observer.call_loader("probe.emit", vec![Value::String("onExtensionResponse".into()), Value::String(serde_json::json!({"type":"json","dataObj":{
        "cmd":"moveToArea","areaId":2,"areaName":"test-1",
        "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],
        "monBranch":[{"MonID":1,"MonMapID":2,"strMonName":"Boss","intHP":50,"intHPMax":50,"intState":1}]
    }}).to_string())]).recv_timeout(TIMEOUT).unwrap();
    supervisor.start_script(&mut session, loose(&[("profile.js", r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.combat.kill("Boss", {profile:{steps:[{skill:1}]}}); }
    "#)])).unwrap();
    while await_event(&observer, "onProbeCall", None).args.first()
        != Some(&Value::String("combat.attackMonster".into()))
    {}
    assert_eq!(session.live_profile_job_count(), 1);
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    let observer = session
        .attach(lucent_host::bridge::EventInterests::ALL)
        .unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&observer);
    logs.wait_for_count(tracing::Level::DEBUG, "session 1: script worker reaped", 2);
    assert_eq!(
        session.attached_link_count().recv_timeout(TIMEOUT).unwrap(),
        fresh_links
    );
    assert_eq!(session.live_profile_job_count(), 0);
    assert_eq!(js_threads(), Vec::<String>::new());
}
