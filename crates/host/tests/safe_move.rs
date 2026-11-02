mod common;
use common::*;
use lucent_host::{Session, Supervisor, bridge::Value, testing};
use lucent_protocol::script::ScriptPhase;
use serde_json::json;
use std::{
    sync::Arc,
    task::{Wake, Waker},
    time::Instant,
};
use tracing_subscriber::prelude::*;

struct Unpark(std::thread::Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn wait_for(session: &Session, predicate: impl Fn() -> bool) {
    session.watch(Waker::from(Arc::new(Unpark(std::thread::current()))));
    let deadline = Instant::now() + TIMEOUT;
    while !predicate() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "session did not settle");
        std::thread::park_timeout(remaining);
    }
}
fn options(session: &Session, enabled: bool) {
    session
        .update_script_options(
            serde_json::from_value(json!({
                "safeStartStop": enabled, "roomPolicy": {"kind":"public"}
            }))
            .unwrap(),
        )
        .unwrap();
}
fn setup(own_house: bool) -> (Session, lucent_host::ScriptLink, Supervisor) {
    setup_with(own_house, |movie| movie)
}
fn setup_with(
    own_house: bool,
    modify: impl FnOnce(testing::ProbeMovie) -> testing::ProbeMovie,
) -> (Session, lucent_host::ScriptLink, Supervisor) {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        modify(
            ready_movie()
                .responding(
                    "flash.callGameFunction0",
                    std::iter::repeat_n((json!(own_house.to_string()), vec![]), 128),
                )
                .recording("auth.logout")
                .constant("flash.isNull", json!(true))
                .constant("combat.cancelAutoAttack", json!(null))
                .constant("combat.cancelTarget", json!(null))
                .raising("flash.callGameFunction", "onDebug", "safe destination"),
        ),
    );
    log_in(&observer);
    options(&session, false);
    let supervisor = Supervisor::start(&mut session, None).unwrap();
    (session, observer, supervisor)
}
#[test]
fn a_killed_script_still_moves_to_safety() {
    let (mut session, observer, mut supervisor) = setup(false);
    let controls = supervisor
        .start_script(
            &mut session,
            loose(&[(
                "x.js",
                r#"
        import * as script from 'lucent/script';
        export default function* () { yield* script.alert('running'); }
    "#,
            )]),
        )
        .unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    options(&session, true);
    controls.kill();
    let event = await_event(&observer, "onDebug", None);
    assert_eq!(event.args, vec![Value::String("safe destination".into())]);
}

#[test]
fn every_terminal_outcome_runs_safe_stop() {
    for ending in ["complete", "stop", "grace", "kill", "exit"] {
        let (layer, gate) = common::hold::hold("stop held", Some("lucent-js-"), ending == "grace");
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(layer));
        let _dispatch = tracing::dispatcher::set_default(&dispatch);
        let (mut session, observer, mut supervisor) = setup(true);
        let source = match ending {
            "grace" => {
                "import * as script from 'lucent/script'; export default function* () { const abort = AbortController.prototype.abort; AbortController.prototype.abort = function(...args) { console.log('stop held'); return abort.apply(this, args); }; yield* script.alert('running'); }"
            }
            "exit" => {
                "import * as script from 'lucent/script'; export default function* () { yield* script.alert('running'); yield* script.exit({ logout: true }); }"
            }
            _ => {
                "import * as script from 'lucent/script'; export default function* () { yield* script.alert('running'); }"
            }
        };
        let controls = supervisor
            .start_script(&mut session, loose(&[("x.js", source)]))
            .unwrap();
        wait_for(&session, || session.script_dialog().is_some());
        options(&session, true);
        match ending {
            "stop" | "grace" => controls.stop(),
            "kill" => controls.kill(),
            _ => session.respond_dialog(
                session.script_dialog().unwrap().id,
                lucent_protocol::script::ScriptDialogResponse::Dismissed,
            ),
        }
        if ending == "grace" {
            gate.wait();
            wait_for(&session, || session.script_status().last_end.is_some());
            assert!(
                session
                    .script_status()
                    .last_end
                    .unwrap()
                    .contains("did not stop within 5 s")
            );
            gate.open();
        }
        wait_for(&session, || {
            session.script_status().phase == ScriptPhase::Idle
        });
        assert!(session.script_status().last_end.is_some(), "{ending}");
        observer
            .call_loader("probe.echo", vec![])
            .recv_timeout(TIMEOUT)
            .unwrap();
        let mut own_house_reads = 0;
        let mut logout_seen = false;
        while let Some(event) = observer.try_event().unwrap() {
            if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
                if event.name == "onProbeCall"
                    && event.args.first() == Some(&Value::String("flash.callGameFunction0".into()))
                {
                    assert!(!logout_seen, "logout preceded safe stop");
                    own_house_reads += 1;
                }
                if event.name == "onProbeCall"
                    && event.args.first() == Some(&Value::String("auth.logout".into()))
                {
                    logout_seen = true;
                }
            }
        }
        assert!(own_house_reads > 0, "{ending}: safe stop was skipped");
        assert_eq!(logout_seen, ending == "exit");
    }
}

#[test]
fn disabled_safe_moves_make_no_house_observation() {
    let (mut session, observer, mut supervisor) = setup(true);
    supervisor
        .start_script(
            &mut session,
            loose(&[("x.js", "export default function* () {}")]),
        )
        .unwrap();
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert!(
                !(event.name == "onProbeCall"
                    && event.args.first()
                        == Some(&Value::String("flash.callGameFunction0".into())))
            );
        }
    }
}

#[test]
fn safe_start_moves_before_the_first_script_line() {
    let (mut session, observer, mut supervisor) = setup_with(false, |movie| {
        movie.responding(
            "flash.callGameFunction0",
            std::iter::once((json!("false"), vec![]))
                .chain(std::iter::repeat_n((json!("true"), vec![]), 128)),
        )
    });
    options(&session, true);
    supervisor
        .start_script(
            &mut session,
            loose(&[(
                "x.js",
                r#"
        import * as api from 'lucent/api';
        export default function* () { yield* api.packet.sendToClient('first'); }
    "#,
            )]),
        )
        .unwrap();
    assert_eq!(
        await_event(&observer, "onDebug", None).args,
        vec![Value::String("safe destination".into())]
    );
    assert_eq!(
        await_event(&observer, "onDebug", None).args,
        vec![Value::String("started".into())]
    );
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
}
#[test]
fn replacing_the_player_during_safe_stop_suppresses_logout() {
    let (mut session, observer, mut supervisor) = setup(false);
    supervisor.start_script(&mut session, loose(&[("x.js", r#"
        import * as script from 'lucent/script';
        export default function* () { yield* script.alert('running'); yield* script.exit({ logout: true }); }
    "#)])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    options(&session, true);
    session.respond_dialog(
        session.script_dialog().unwrap().id,
        lucent_protocol::script::ScriptDialogResponse::Dismissed,
    );
    await_event(&observer, "onDebug", None);
    assert_eq!(session.script_status().phase, ScriptPhase::Finishing);
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    while let Ok(Some(event)) = observer.try_event() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert!(
                !(event.name == "onProbeCall"
                    && event.args.first() == Some(&Value::String("auth.logout".into())))
            );
        }
    }
}

#[test]
fn cancelling_safe_start_waits_for_restoration_and_sends_no_more_movement() {
    let (mut session, observer, mut supervisor) = setup_with(false, |movie| {
        movie
            .constant("flash.isNull", json!(false))
            .constant("flash.getGameObject", json!("{\"map\":\"old\"}"))
            .responding(
                "flash.setGameObject",
                [
                    (json!(null), vec![]),
                    (json!(null), vec![("onDebug".into(), "restored".into())]),
                ],
            )
    });
    options(&session, true);
    let controls = supervisor
        .start_script(
            &mut session,
            loose(&[(
                "x.js",
                "export default function* () { throw new Error('must not boot'); }",
            )]),
        )
        .unwrap();
    assert_eq!(
        await_event(&observer, "onDebug", None).args,
        vec![Value::String("safe destination".into())]
    );
    let (entered, arrived) = std::sync::mpsc::channel();
    let (release, held) = std::sync::mpsc::channel::<()>();
    let _hold = session.read_projection(move |_| {
        let _ = entered.send(());
        let _ = held.recv();
    });
    arrived.recv_timeout(TIMEOUT).unwrap();
    options(&session, false);
    controls.stop();
    let result = supervisor.start_script(
        &mut session,
        loose(&[("next.js", "export default function* () {}")]),
    );
    drop(release);
    assert!(matches!(
        result,
        Err(lucent_host::scripting::RuntimeError::Busy)
    ));
    assert_eq!(
        await_event(&observer, "onDebug", None).args,
        vec![Value::String("restored".into())]
    );
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert_ne!(event.name, "onDebug", "a cancelled move continued");
        }
    }
}

#[test]
fn changing_accounts_during_safe_stop_suppresses_logout() {
    let (mut session, observer, mut supervisor) = setup_with(false, |movie| {
        movie.responding(
            "flash.getGameObjectS",
            [
                (json!("\"player-a\""), vec![]),
                (json!("\"player-b\""), vec![]),
            ],
        )
    });
    supervisor.start_script(&mut session, loose(&[("x.js", r#"
        import * as script from 'lucent/script';
        export default function* () { yield* script.alert('running'); yield* script.exit({ logout: true }); }
    "#)])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    options(&session, true);
    session.respond_dialog(
        session.script_dialog().unwrap().id,
        lucent_protocol::script::ScriptDialogResponse::Dismissed,
    );
    await_event(&observer, "onDebug", None);
    observer
        .command(lucent_protocol::command::Command::RefreshCredentials {})
        .recv_timeout(TIMEOUT)
        .unwrap();
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            assert!(
                !(event.name == "onProbeCall"
                    && event.args.first() == Some(&Value::String("auth.logout".into())))
            );
            assert_ne!(event.name, "onDebug", "an invalidated safe move continued");
        }
    }
}

#[test]
fn a_specific_buyhouse_move_finishes_before_boot() {
    let packet = json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":2,"areaName":"buyhouse-42", "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}}).to_string();
    let (mut session, observer, mut supervisor) = setup_with(true, |movie| {
        movie
            .constant("world.isActionAvailable", json!(true))
            .responding("flash.sendClientPacket", [(json!(null), vec![])])
            .responding(
                "player.joinMap",
                [(json!(null), vec![("onExtensionResponse".into(), packet)])],
            )
    });
    observer.call_loader("probe.emit", vec![Value::String("onExtensionResponse".into()), Value::String(json!({"type":"json","dataObj":{"cmd":"loadInventoryBig","items":[],"hitems":[]}}).to_string())]).recv_timeout(TIMEOUT).unwrap();
    session
        .update_script_options(
            serde_json::from_value(
                json!({"safeStartStop":true,"roomPolicy":{"kind":"specific","roomNumber":42}}),
            )
            .unwrap(),
        )
        .unwrap();
    supervisor
        .start_script(
            &mut session,
            loose(&[(
                "x.js",
                r#"
        import * as api from 'lucent/api';
        export default function* () { yield* api.packet.sendToClient('first'); }
    "#,
            )]),
        )
        .unwrap();
    let mut joined = false;
    loop {
        let event = await_event(&observer, "onProbeCall", None);
        if event.args[0] == Value::String("player.joinMap".into()) {
            assert!(!joined);
            joined = true;
        }
        if event.args[0] == Value::String("flash.sendClientPacket".into()) {
            break;
        }
    }
    assert!(joined);
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    assert_eq!(observer.read(|p| p.world.map.room_number).unwrap(), 42.0);
}

#[test]
fn the_next_run_waits_for_safe_stop_and_reaping() {
    let (mut session, observer, mut supervisor) = setup_with(false, |movie| {
        movie.responding(
            "flash.callGameFunction0",
            std::iter::once((json!("false"), vec![]))
                .chain(std::iter::repeat_n((json!("true"), vec![]), 128)),
        )
    });
    let controls = supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.alert('running'); }")])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    let (entered, arrived) = std::sync::mpsc::channel();
    let (release, held) = std::sync::mpsc::channel::<()>();
    lucent_host::reap(move || {
        let _ = entered.send(());
        let _ = held.recv();
    });
    arrived.recv_timeout(TIMEOUT).unwrap();
    options(&session, true);
    controls.kill();
    let destination = await_event(&observer, "onDebug", None);
    let result = supervisor.start_script(
        &mut session,
        loose(&[("next.js", "export default function* () {}")]),
    );
    drop(release);
    assert_eq!(
        destination.args,
        vec![Value::String("safe destination".into())]
    );
    assert!(matches!(
        result,
        Err(lucent_host::scripting::RuntimeError::Busy)
    ));
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    supervisor
        .start_script(
            &mut session,
            loose(&[("next.js", "export default function* () {}")]),
        )
        .unwrap();
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
}

#[test]
fn a_failed_logout_is_reported_once_and_finishes_the_run() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _dispatch = tracing::dispatcher::set_default(&dispatch);
    let (mut session, _observer, mut supervisor) = setup_with(true, |movie| {
        movie.throwing("auth.logout", "logout rejected")
    });
    supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.exit({ logout: true }); }")])).unwrap();
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    assert_eq!(
        logs.messages(tracing::Level::WARN)
            .iter()
            .filter(|message| message.contains("logout failed:"))
            .count(),
        1
    );
}

#[test]
fn changing_the_equipped_house_during_transfer_does_not_hide_arrival() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _dispatch = tracing::dispatcher::set_default(&dispatch);
    let packet = json!({"type":"json","dataObj":{"cmd":"loadInventoryBig","items":[],"hitems":[]}})
        .to_string();
    let (mut session, observer, mut supervisor) = setup_with(false, |movie| {
        movie
            .responding(
                "flash.callGameFunction0",
                std::iter::once((json!("false"), vec![]))
                    .chain(std::iter::repeat_n((json!("true"), vec![]), 128)),
            )
            .responding(
                "flash.callGameFunction",
                [(json!(null), vec![("onExtensionResponse".into(), packet)])],
            )
    });
    options(&session, true);
    supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.alert('arrived'); }")])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    assert_eq!(session.script_dialog().unwrap().message, "arrived");
    assert!(
        !logs
            .messages(tracing::Level::WARN)
            .iter()
            .any(|message| message.contains("safe move failed:")),
        "{:?}",
        logs.messages(tracing::Level::WARN)
    );
    options(&session, false);
    session.respond_dialog(
        session.script_dialog().unwrap().id,
        lucent_protocol::script::ScriptDialogResponse::Dismissed,
    );
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    assert!(
        observer
            .read(|p| p
                .items
                .all(lucent_protocol::item::ItemContainer::House)
                .is_empty())
            .unwrap()
    );
}

#[test]
fn a_safe_move_loader_error_is_reported_once_and_does_not_block_boot() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _dispatch = tracing::dispatcher::set_default(&dispatch);
    let (mut session, _observer, mut supervisor) = setup_with(true, |movie| {
        movie.throwing("flash.callGameFunction0", "house unavailable")
    });
    options(&session, true);
    supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.alert('running'); }")])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    assert_eq!(
        logs.messages(tracing::Level::WARN)
            .iter()
            .filter(|message| message.contains("safe move failed:"))
            .count(),
        1
    );
    options(&session, false);
    session.respond_dialog(
        session.script_dialog().unwrap().id,
        lucent_protocol::script::ScriptDialogResponse::Dismissed,
    );
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
}
