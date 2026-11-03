mod common;
use common::*;
use lucent_host::{
    Session, Supervisor,
    bridge::{EventInterests, ScriptEvent, Value},
    testing,
};
use lucent_protocol::script::ScriptPhase;
use serde_json::json;
use std::{
    sync::Arc,
    task::{Wake, Waker},
    time::{Duration, Instant},
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
fn a_script_ended_before_it_starts_makes_no_safe_move() {
    for kill in [false, true] {
        let (mut session, observer, mut supervisor) = setup_with(true, |movie| {
            movie.responding(
                "player.isLoaded",
                std::iter::repeat_n((json!(false), vec![]), 256),
            )
        });
        let (calls, observed) = std::sync::mpsc::channel();
        session.set_safe_move_probe(move |issued| {
            let _ = calls.send(issued);
        });
        options(&session, true);
        let controls = supervisor
            .start_script(
                &mut session,
                loose(&[("x.js", "export default function* () {}")]),
            )
            .unwrap();
        assert_eq!(session.script_status().phase, ScriptPhase::Waiting);
        if kill {
            controls.kill();
        } else {
            controls.stop();
        }
        wait_for(&session, || {
            session.script_status().phase == ScriptPhase::Idle
        });
        let events = drain(&observer);
        let end = if kill {
            "x.js was killed"
        } else {
            "x.js was stopped"
        };
        assert_eq!(session.script_status().last_end.as_deref(), Some(end));
        assert_eq!(probe_calls(&events, "flash.callGameFunction0"), 0);
        assert!(observed.try_recv().is_err(), "a safe move ran");
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
    wait_for(&session, || {
        session.script_status().last_end.is_some()
            && session.script_status().phase == ScriptPhase::Idle
    });
    let events = drain(&observer);
    let order: Vec<_> = events
        .iter()
        .filter_map(|(name, args)| match (name.as_str(), args.first()) {
            ("onProbeCall", Some(Value::String(method))) if method == "flash.callGameFunction0" => {
                Some("house read")
            }
            ("onDebug", Some(Value::String(text))) if text == "safe destination" => Some("move"),
            ("onDebug", Some(Value::String(text))) if text == "started" => Some("first line"),
            _ => None,
        })
        .collect();
    let moved = order.iter().position(|event| *event == "move").unwrap();
    let first = order
        .iter()
        .position(|event| *event == "first line")
        .unwrap();
    assert!(moved < first, "{order:?}");
    assert!(order[moved..first].contains(&"house read"), "{order:?}");
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
    let fresh = session.attach(EventInterests::ALL).unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&fresh);
    log_in(&fresh);
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
    let fresh_events = drain(&fresh);
    assert_eq!(probe_calls(&fresh_events, "auth.logout"), 0);
    assert_eq!(probe_calls(&fresh_events, "flash.callGameFunction0"), 0);
    assert_eq!(probe_calls(&fresh_events, "flash.callGameFunction"), 0);
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
    let (layer, gate) = common::hold::hold("script worker joining", Some("lucent-reaper"), true);
    let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(layer));
    let _dispatch = tracing::dispatcher::set_default(&dispatch);
    let (mut session, observer, mut supervisor) = setup_with(false, |movie| {
        movie.responding(
            "flash.callGameFunction0",
            std::iter::once((json!("false"), vec![]))
                .chain(std::iter::repeat_n((json!("true"), vec![]), 128)),
        )
    });
    let controls = supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.alert('running'); }")])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    options(&session, true);
    controls.kill();
    gate.wait();
    let destination = await_event(&observer, "onDebug", None);
    let result = supervisor.start_script(
        &mut session,
        loose(&[("next.js", "export default function* () {}")]),
    );
    gate.open();
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

#[test]
fn failed_native_joins_retry_before_safe_start_finishes() {
    let (mut session, observer, mut supervisor) = setup_with(true, |movie| {
        movie
            .constant("world.isActionAvailable", json!(true))
            .throwing("player.joinMap", "join rejected")
            .recording("player.joinMap")
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
    supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.alert('running'); }")])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    options(&session, false);
    session.respond_dialog(
        session.script_dialog().unwrap().id,
        lucent_protocol::script::ScriptDialogResponse::Dismissed,
    );
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    let mut joins = 0;
    while let Some(event) = observer.try_event().unwrap() {
        if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event {
            joins += usize::from(
                event.name == "onProbeCall"
                    && event.args.first() == Some(&Value::String("player.joinMap".into())),
            );
        }
    }
    assert_eq!(joins, 3);
}

#[test]
fn changing_accounts_withholds_saved_return_info_from_the_new_account() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _dispatch = tracing::dispatcher::set_default(&dispatch);
    let (mut session, observer, mut supervisor) = setup_with(false, restoring_movie);
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
    let refreshed = observer.command(lucent_protocol::command::Command::RefreshCredentials {});
    controls.stop();
    drop(release);
    refreshed.recv_timeout(TIMEOUT).unwrap();
    wait_for(&session, || {
        session.script_status().last_end.is_some()
            && session.script_status().phase == ScriptPhase::Idle
    });
    let events = drain(&observer);
    eprintln!(
        "end={:?} restored={} setGameObject calls={} warns={:?}",
        session.script_status().last_end,
        debug(&events, "restored"),
        probe_calls(&events, "flash.setGameObject"),
        logs.messages(tracing::Level::WARN)
    );
    assert_eq!(debug(&events, "safe destination"), 0);
    assert_eq!(debug(&events, "restored"), 0);
}

#[test]
fn cancelling_safe_start_issues_no_further_game_calls() {
    let (mut session, observer, mut supervisor) = setup_with(false, |movie| {
        movie
            .recording("world.isLoaded")
            .recording("player.isLoaded")
            .responding(
                "flash.callGameFunction",
                std::iter::repeat_n((json!(null), vec![]), 16),
            )
    });
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let after_cancel = cancelled.clone();
    let (calls, recorded) = std::sync::mpsc::channel();
    session.set_safe_move_probe(move |issued| {
        let _ = calls.send((
            after_cancel.load(std::sync::atomic::Ordering::Acquire),
            issued,
        ));
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
    loop {
        let event = await_event(&observer, "onProbeCall", None);
        if event.args.first() == Some(&Value::String("flash.callGameFunction".into())) {
            break;
        }
    }
    let (entered, arrived) = std::sync::mpsc::channel();
    let (release, held) = std::sync::mpsc::channel::<()>();
    let _hold = session.read_projection(move |_| {
        let _ = entered.send(());
        let _ = held.recv();
    });
    arrived.recv_timeout(TIMEOUT).unwrap();
    while let Ok(Some(_)) = observer.try_event() {}
    options(&session, false);
    controls.stop();
    wait_for(&session, || session.script_status().last_end.is_some());
    cancelled.store(true, std::sync::atomic::Ordering::Release);
    drop(release);
    session.watch(Waker::from(Arc::new(Unpark(std::thread::current()))));
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    while session.script_status().phase != ScriptPhase::Idle {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "did not settle");
        std::thread::park_timeout(remaining);
    }
    drain(&observer);
    let observations: Vec<_> = recorded.try_iter().collect();
    assert!(observations.iter().any(|(cancelled, calls)| {
        !cancelled
            && calls
                .iter()
                .any(|method| method == "flash.callGameFunction")
    }));
    let calls: Vec<_> = observations
        .into_iter()
        .filter(|(cancelled, _)| *cancelled)
        .flat_map(|(_, calls)| calls)
        .collect();
    assert!(calls.is_empty(), "{calls:?}");
}

fn drain(link: &lucent_host::ScriptLink) -> Vec<(String, Vec<Value>)> {
    link.call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    let mut events = Vec::new();
    while let Ok(Some(event)) = link.try_event() {
        if let ScriptEvent::Loader { event, .. } = event {
            events.push((event.name, event.args));
        }
    }
    events
}

fn probe_calls(events: &[(String, Vec<Value>)], method: &str) -> usize {
    events
        .iter()
        .filter(|(name, args)| {
            name == "onProbeCall" && args.first() == Some(&Value::String(method.into()))
        })
        .count()
}

fn debug(events: &[(String, Vec<Value>)], text: &str) -> usize {
    events
        .iter()
        .filter(|(name, args)| {
            name == "onDebug" && args.first() == Some(&Value::String(text.into()))
        })
        .count()
}

fn restoring_movie(movie: testing::ProbeMovie) -> testing::ProbeMovie {
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
        .responding(
            "flash.getGameObjectS",
            [
                (json!("\"player-a\""), vec![]),
                (json!("\"player-b\""), vec![]),
            ],
        )
}

#[test]
fn unconfirmed_house_arrival_retries_with_backoff_before_boot() {
    let (mut session, observer, mut supervisor) = setup_with(false, |movie| {
        movie
            .responding(
                "flash.callGameFunction0",
                std::iter::repeat_n((json!("false"), vec![]), 512),
            )
            .responding(
                "flash.callGameFunction",
                std::iter::repeat_n((json!(null), vec![]), 16),
            )
    });
    let (attempted, times) = std::sync::mpsc::channel();
    session.set_safe_move_probe(move |calls| {
        if calls
            .iter()
            .any(|method| method == "flash.callGameFunction")
        {
            let _ = attempted.send(Instant::now());
        }
    });
    options(&session, true);
    supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.alert('booted'); }")])).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    session.watch(Waker::from(Arc::new(Unpark(std::thread::current()))));
    while session.script_dialog().is_none() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "did not boot after house retries");
        std::thread::park_timeout(remaining);
    }
    options(&session, false);
    session.respond_dialog(
        session.script_dialog().unwrap().id,
        lucent_protocol::script::ScriptDialogResponse::Dismissed,
    );
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    let events = drain(&observer);
    let packets: Vec<_> = events
        .iter()
        .filter(|(name, args)| {
            name == "onProbeCall"
                && args.first() == Some(&Value::String("flash.callGameFunction".into()))
        })
        .collect();
    assert_eq!(packets.len(), 3);
    assert!(
        packets
            .iter()
            .all(|(_, args)| format!("{args:?}").contains("%xt%zm%house%1%player-a%"))
    );
    let attempts: Vec<_> = times.try_iter().collect();
    assert_eq!(attempts.len(), 3, "the house move must make three attempts");
    assert!(attempts[1].duration_since(attempts[0]) >= Duration::from_millis(5800));
    assert!(attempts[2].duration_since(attempts[1]) >= Duration::from_millis(6800));
}

#[test]
fn changing_accounts_while_running_never_moves_the_new_account() {
    let (mut session, observer, mut supervisor) = setup_with(false, |movie| {
        movie
            .responding(
                "flash.getGameObjectS",
                [
                    (json!("\"player-a\""), vec![]),
                    (json!("\"player-b\""), vec![]),
                ],
            )
            .responding(
                "flash.callGameFunction",
                std::iter::repeat_n((json!(null), vec![]), 16),
            )
    });
    supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.alert('running'); }")])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    options(&session, true);
    drain(&observer);
    observer
        .command(lucent_protocol::command::Command::RefreshCredentials {})
        .recv_timeout(TIMEOUT)
        .unwrap();
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    assert!(
        session
            .script_status()
            .last_end
            .unwrap()
            .contains("Account changed")
    );
    let events = drain(&observer);
    assert_eq!(
        probe_calls(&events, "flash.callGameFunction"),
        0,
        "{events:?}"
    );
    assert_eq!(
        probe_calls(&events, "flash.callGameFunction0"),
        0,
        "{events:?}"
    );
}

#[test]
fn logged_out_safe_stop_makes_no_game_calls() {
    let (mut session, observer, mut supervisor) = setup(false);
    supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.alert('running'); }")])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    let (calls, recorded) = std::sync::mpsc::channel();
    session.set_safe_move_probe(move |issued| {
        let _ = calls.send(issued);
    });
    options(&session, true);
    for command in [
        lucent_protocol::command::Command::Logout {},
        lucent_protocol::command::Command::RefreshCredentials {},
        lucent_protocol::command::Command::RefreshLoggedIn {},
    ] {
        observer.command(command).recv_timeout(TIMEOUT).unwrap();
    }
    assert_eq!(
        observer
            .read(|p| (p.auth.logged_in, p.auth.username.clone()))
            .unwrap(),
        (true, "player-a".into())
    );
    drain(&observer);
    session.respond_dialog(
        session.script_dialog().unwrap().id,
        lucent_protocol::script::ScriptDialogResponse::Dismissed,
    );
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    let events = drain(&observer);
    let observations: Vec<_> = recorded.try_iter().collect();
    assert!(!observations.is_empty(), "safe stop was skipped");
    let calls: Vec<_> = observations.into_iter().flatten().collect();
    assert_eq!(probe_calls(&events, "flash.callGameFunction0"), 0);
    assert!(calls.is_empty(), "{calls:?}");
}

#[test]
fn disconnected_safe_stop_ignores_a_stale_logged_in_client() {
    let (mut session, observer, mut supervisor) = setup(false);
    supervisor.start_script(&mut session, loose(&[("x.js", "import * as script from 'lucent/script'; export default function* () { yield* script.alert('running'); }")])).unwrap();
    wait_for(&session, || session.script_dialog().is_some());
    let (calls, observed) = std::sync::mpsc::channel();
    session.set_safe_move_probe(move |issued| {
        let _ = calls.send(issued);
    });
    options(&session, true);
    let (entered, arrived) = std::sync::mpsc::channel();
    let (release, held) = std::sync::mpsc::channel::<()>();
    let _hold = session.read_projection(move |_| {
        let _ = entered.send(());
        let _ = held.recv();
    });
    arrived.recv_timeout(TIMEOUT).unwrap();
    let disconnected = observer.call_loader(
        "probe.emit",
        vec![
            Value::String("onConnection".into()),
            Value::String("OnConnectionLost".into()),
        ],
    );
    let credentials = observer.command(lucent_protocol::command::Command::RefreshCredentials {});
    let logged_in = observer.command(lucent_protocol::command::Command::RefreshLoggedIn {});
    drop(release);
    disconnected.recv_timeout(TIMEOUT).unwrap();
    credentials.recv_timeout(TIMEOUT).unwrap();
    logged_in.recv_timeout(TIMEOUT).unwrap();
    assert!(observer.read(|p| p.auth.logged_in).unwrap());
    let issued = observed.recv_timeout(TIMEOUT).unwrap();
    assert!(
        issued.is_empty(),
        "disconnected cleanup called the game: {issued:?}"
    );
    wait_for(&session, || {
        session.script_status().phase == ScriptPhase::Idle
    });
    assert!(observed.try_iter().flatten().next().is_none());
    assert!(
        session
            .script_status()
            .last_end
            .unwrap()
            .contains("Connection lost")
    );
}
