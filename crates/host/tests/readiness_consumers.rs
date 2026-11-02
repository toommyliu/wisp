mod common;

use common::*;
use lucent_host::{
    ScriptLink,
    bridge::{ScriptEvent, Value},
    testing,
};
use lucent_protocol::{command::Command, script::ScriptOutcome};
use serde_json::json;
use std::collections::BTreeMap;

fn emit(link: &ScriptLink, name: &str, data: impl Into<String>) {
    link.call_loader(
        "probe.emit",
        vec![Value::String(name.into()), Value::String(data.into())],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
    barrier(link);
}

fn barrier(link: &ScriptLink) {
    link.call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
}

fn login(link: &ScriptLink) {
    emit(link, "onConnection", "OnConnection");
    facts(link);
}

fn facts(link: &ScriptLink) {
    for data in [
        json!({"cmd":"loadInventoryBig","items":[],"hitems":[]}),
        json!({"cmd":"moveToArea","areaId":1,"areaName":"battleon-1",
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}),
    ] {
        emit(
            link,
            "onExtensionResponse",
            json!({"type":"json","dataObj":data}).to_string(),
        );
    }
}

fn next(link: &ScriptLink, deadline: std::time::Instant) -> ScriptEvent {
    use std::{
        sync::Arc,
        task::{Wake, Waker},
    };
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    link.set_waker(Waker::from(Arc::new(Unpark(std::thread::current()))));
    loop {
        if let Some(event) = link.try_event().unwrap() {
            return event;
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "no readiness consumer progress before deadline"
        );
        std::thread::park_timeout(remaining);
    }
}

fn calls(link: &ScriptLink) -> BTreeMap<String, usize> {
    barrier(link);
    let mut calls = BTreeMap::new();
    while let Some(event) = link.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
            && let Some(Value::String(method)) = event.args.first()
        {
            *calls.entry(method.clone()).or_default() += 1;
        }
    }
    calls
}

fn counted_movie() -> testing::ProbeMovie {
    ready_movie()
        .recording("auth.isLoggedIn")
        .recording("world.isLoaded")
        .recording("player.isLoaded")
}

#[test]
fn a_waiting_script_and_armed_relogin_share_avatar_confirmation() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        counted_movie()
            .constant("flash.isNull", json!(true))
            .recording("settings.setWalkSpeed")
            .responding(
                "player.isLoaded",
                [false, false, true].map(|value| (json!(value), vec![])),
            ),
    );
    session
        .update_script_options(serde_json::from_value(json!({"safeStartStop":false})).unwrap())
        .unwrap();
    session.supervise(None).unwrap();
    observer
        .command(
            serde_json::from_value(json!({"kind":"updateSettings","patch":{"walkSpeed":30}}))
                .unwrap(),
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
    session
        .start_script(loose(&[(
            "ready.js",
            r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.packet.sendToClient("started"); }
    "#,
        )]))
        .unwrap();
    login(&observer);
    observer
        .command(Command::SetAutoReloginEnabled { enabled: true })
        .recv_timeout(TIMEOUT)
        .unwrap();
    let mut recorded = BTreeMap::new();
    let mut started = false;
    let mut reapplies = 0;
    let deadline = std::time::Instant::now() + TIMEOUT;
    while !started || reapplies < 3 {
        if let ScriptEvent::Loader { event, .. } = next(&observer, deadline) {
            started |= event.name == "onDebug";
            if event.name == "onProbeCall"
                && let Some(Value::String(method)) = event.args.first()
            {
                *recorded.entry(method.clone()).or_default() += 1;
                reapplies += usize::from(method == "settings.setWalkSpeed");
            }
        }
    }
    for (method, count) in calls(&observer) {
        *recorded.entry(method).or_default() += count;
    }
    assert_eq!(
        recorded.get("auth.isLoggedIn").copied().unwrap_or(0),
        0,
        "{recorded:?}"
    );
    assert_eq!(
        recorded.get("world.isLoaded").copied().unwrap_or(0),
        0,
        "{recorded:?}"
    );
    assert_eq!(recorded.get("player.isLoaded"), Some(&3), "{recorded:?}");
    assert_eq!(
        session.auto_relogin().status.phase,
        lucent_protocol::auto_relogin::AutoReloginPhase::Armed
    );
}

#[test]
fn script_readiness_reads_the_record_and_rejects_logout() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, counted_movie());
    session
        .update_script_options(serde_json::from_value(json!({"safeStartStop":false})).unwrap())
        .unwrap();
    login(&observer);
    await_event(&observer, "onProbeCall", None);
    barrier(&observer);
    calls(&observer);
    let script = loose(&[(
        "ready.js",
        r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            const before = yield* api.player.isReady();
            yield* api.auth.logout();
            const after = yield* api.player.isReady();
            yield* script.stop(`${before}/${after}`);
        }
    "#,
    )]);
    let run = start(&mut session, script, Default::default());
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true/false".into())
        }
    );
    let recorded = calls(&observer);
    for method in ["auth.isLoggedIn", "world.isLoaded", "player.isLoaded"] {
        assert_eq!(
            recorded.get(method).copied().unwrap_or(0),
            0,
            "{recorded:?}"
        );
    }
}

#[test]
fn login_on_the_same_socket_releases_logout_readiness() {
    let host = testing::headless_host();
    let (_session, observer) =
        testing::start_probe(&host, counted_movie().constant("auth.login", json!(null)));
    login(&observer);
    await_event(&observer, "onProbeCall", None);
    barrier(&observer);
    observer
        .command(Command::Logout {})
        .recv_timeout(TIMEOUT)
        .unwrap();
    assert!(!observer.read(|p| p.readiness.can_act()).unwrap());
    observer
        .command(Command::Login {
            username: "player-a".into(),
            password: "pw".into(),
        })
        .recv_timeout(TIMEOUT)
        .unwrap();
    let deadline = std::time::Instant::now() + TIMEOUT;
    while !observer.read(|p| p.readiness.can_act()).unwrap() {
        next(&observer, deadline);
    }
}

#[test]
fn travel_uses_new_avatar_readiness_after_each_join() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        counted_movie()
            .constant("world.isActionAvailable", json!(true))
            .responding(
                "player.isLoaded",
                [true, false, false, true].map(|value| (json!(value), vec![])),
            )
            .responding(
                "player.joinMap",
                [(
                    json!(null),
                    vec![("onExtensionResponse".into(), json!({
            "type":"json", "dataObj":{"cmd":"moveToArea","areaId":2,"areaName":"doom-1",
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}
        }).to_string())],
                )],
            ),
    );
    login(&observer);
    await_event(&observer, "onProbeCall", None);
    barrier(&observer);
    calls(&observer);
    session
        .update_script_options(
            serde_json::from_value(json!({"roomPolicy":{"kind":"public"}})).unwrap(),
        )
        .unwrap();
    session
        .update_script_options(serde_json::from_value(json!({"safeStartStop":false})).unwrap())
        .unwrap();
    session.supervise(None).unwrap();
    let run = start(
        &mut session,
        loose(&[(
            "travel.js",
            r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () { yield* script.stop(String(yield* api.player.joinMap("doom"))); }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(
        run.wait(TIMEOUT).unwrap(),
        ScriptOutcome::StoppedByScript {
            reason: Some("true".into())
        }
    );
    assert!(
        observer.read(|p| p.readiness.can_act()).unwrap(),
        "travel returned before the new avatar loaded"
    );
    let recorded = calls(&observer);
    assert_eq!(
        recorded.get("auth.isLoggedIn").copied().unwrap_or(0),
        0,
        "{recorded:?}"
    );
    assert_eq!(recorded.get("world.isLoaded"), Some(&2), "{recorded:?}");
    assert_eq!(recorded.get("player.isLoaded"), Some(&3), "{recorded:?}");
}

#[test]
fn drop_accept_uses_host_readiness_without_refreshing_login() {
    use lucent_protocol::{
        operation::{Operation, OperationAnswer},
        query::ItemSelector,
    };
    let host = testing::headless_host();
    let (_session, observer) = testing::start_probe(
        &host,
        counted_movie().accepting_drop(
            std::time::Duration::ZERO,
            json!({"type":"json","dataObj":{
                "cmd":"getDrop","ItemID":70,"bSuccess":true,"iQtyNow":1
            }})
            .to_string(),
        ),
    );
    let mut recorded = BTreeMap::new();
    for stage in 0..3 {
        if stage == 1 {
            login(&observer);
            let deadline = std::time::Instant::now() + TIMEOUT;
            while !observer.read(|p| p.readiness.can_act()).unwrap() {
                next(&observer, deadline);
            }
        } else if stage == 2 {
            observer
                .command(Command::Logout {})
                .recv_timeout(TIMEOUT)
                .unwrap();
        }
        emit(
            &observer,
            "onExtensionResponse",
            json!({"type":"json","dataObj":{
                "cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Drop","iQty":1}}
            }})
            .to_string(),
        );
        let id = observer
            .operate(Operation::AcceptDrop {
                selector: ItemSelector::Id(70.0),
            })
            .unwrap();
        let deadline = std::time::Instant::now() + TIMEOUT;
        loop {
            match next(&observer, deadline) {
                ScriptEvent::Operation {
                    id: received,
                    answer,
                } => {
                    assert_eq!(received, id);
                    assert_eq!(answer.unwrap(), OperationAnswer::AcceptDrop(stage == 1));
                    break;
                }
                ScriptEvent::Loader { event, .. } if event.name == "onProbeCall" => {
                    if let Some(Value::String(method)) = event.args.first() {
                        *recorded.entry(method.clone()).or_default() += 1;
                    }
                }
                _ => {}
            }
        }
    }
    for (method, count) in calls(&observer) {
        *recorded.entry(method).or_default() += count;
    }
    assert_eq!(
        recorded.get("auth.isLoggedIn").copied().unwrap_or(0),
        0,
        "{recorded:?}"
    );
}

#[test]
fn failed_login_preserves_logout_readiness() {
    let host = testing::headless_host();
    let (_session, observer) = testing::start_probe(
        &host,
        counted_movie().throwing("auth.login", "login failed"),
    );
    login(&observer);
    await_event(&observer, "onProbeCall", None);
    barrier(&observer);
    observer
        .command(Command::Logout {})
        .recv_timeout(TIMEOUT)
        .unwrap();
    assert!(
        observer
            .command(Command::Login {
                username: "player-a".into(),
                password: "pw".into()
            })
            .recv_timeout(TIMEOUT)
            .is_err()
    );
    barrier(&observer);
    assert!(!observer.read(|p| p.readiness.can_act()).unwrap());
}

#[test]
fn a_wanted_drop_is_reconsidered_when_the_avatar_becomes_ready() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        counted_movie().accepting_drop(
            std::time::Duration::ZERO,
            json!({"type":"json","dataObj":{"cmd":"getDrop","ItemID":70,"bSuccess":true,"iQtyNow":1}})
                .to_string(),
        ),
    );
    observer
        .command(
            serde_json::from_value(
                json!({"kind":"updateEnvironment","edit":{"kind":"addItem","name":"Wanted"}}),
            )
            .unwrap(),
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
    emit(&observer, "onConnection", "OnConnection");
    emit(
        &observer,
        "onExtensionResponse",
        json!({"type":"json","dataObj":{
            "cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Wanted","iQty":1}}
        }})
        .to_string(),
    );
    let drops = |link: &ScriptLink| {
        link.read(|p| {
            p.items
                .all(lucent_protocol::item::ItemContainer::Drop)
                .len()
        })
        .unwrap()
    };
    session.supervise(None).unwrap();
    logs.wait_for(tracing::Level::DEBUG, "environment drops reconciled");
    assert_eq!(drops(&observer), 1);
    facts(&observer);
    let deadline = std::time::Instant::now() + TIMEOUT;
    while drops(&observer) != 0 {
        next(&observer, deadline);
    }
}

#[test]
fn rejected_login_preserves_logout_readiness() {
    let host = testing::headless_host();
    let (_session, observer) = testing::start_probe(&host, counted_movie().recording("auth.login"));
    login(&observer);
    let deadline = std::time::Instant::now() + TIMEOUT;
    while !observer.read(|p| p.readiness.can_act()).unwrap() {
        next(&observer, deadline);
    }
    observer
        .command(Command::Logout {})
        .recv_timeout(TIMEOUT)
        .unwrap();
    calls(&observer);
    assert_eq!(
        observer
            .command(Command::Login {
                username: " ".into(),
                password: "pw".into()
            })
            .recv_timeout(TIMEOUT)
            .unwrap(),
        "false"
    );
    assert!(!observer.read(|p| p.readiness.can_act()).unwrap());
    assert_eq!(calls(&observer).get("auth.login"), None);
}

fn wait_session(session: &lucent_host::Session, predicate: impl Fn() -> bool) {
    use std::{
        sync::{Arc, mpsc},
        task::{Wake, Waker},
    };
    struct Notice(mpsc::Sender<()>);
    impl Wake for Notice {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }
    let (send, receive) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Notice(send))));
    let deadline = std::time::Instant::now() + TIMEOUT;
    while !predicate() {
        receive
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .unwrap();
    }
}

#[test]
fn cancelled_readiness_waiters_never_boot_or_move_after_login() {
    use lucent_host::{
        ViewEvent, Viewport,
        bridge::EventInterests,
        lifecycle::{KillReason, Outcome, RunPhase, StopReason},
    };
    for action in ["stop", "kill", "restart"] {
        let logs = common::logs::Logs::default();
        let dispatch = logs.dispatch();
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let host = testing::headless_host();
        let movie = counted_movie()
            .recording("player.joinMap")
            .recording("player.jumpToCell")
            .recording("combat.cancelAutoAttack")
            .recording("flash.callGameFunction0");
        let (mut session, mut observer) = testing::start_probe(&host, movie);
        session.supervise(None).unwrap();
        session
            .start_script(loose(&[(
                "waiting.js",
                "export default function* () { console.log('waiting body'); }",
            )]))
            .unwrap();
        assert_eq!(
            session.script_lifecycle().active.unwrap().phase,
            RunPhase::Waiting
        );
        let waiting = session.script_lifecycle().active.unwrap().run;
        match action {
            "stop" => session.stop_script(),
            "kill" => session.kill_script(),
            _ => {
                session.restart_player(testing::headless_surface()).unwrap();
                observer = session.attach(EventInterests::ALL).unwrap();
                session.send(ViewEvent::Resized(
                    Viewport::new(100.0, 100.0, 1.0).unwrap(),
                ));
                testing::wait_loaded(&observer);
            }
        }
        wait_session(&session, || session.script_lifecycle().active.is_none());
        let ended = session.script_lifecycle().latest.unwrap();
        assert_eq!(ended.run, waiting, "{action}");
        assert!(
            match action {
                "stop" => matches!(ended.outcome, Outcome::Stopped(StopReason::Requested(_))),
                "kill" => matches!(ended.outcome, Outcome::Killed(KillReason::Requested)),
                _ => matches!(ended.outcome, Outcome::Stopped(StopReason::PlayerReplaced)),
            },
            "{action}: {:?}",
            ended.outcome
        );
        login(&observer);
        let deadline = std::time::Instant::now() + TIMEOUT;
        while !observer.read(|p| p.readiness.can_act()).unwrap() {
            next(&observer, deadline);
        }
        let recorded = calls(&observer);
        session
            .update_script_options(serde_json::from_value(json!({"safeStartStop":false})).unwrap())
            .unwrap();
        session
            .start_script(loose(&[(
                "after.js",
                "export default function* () { console.log('after body'); }",
            )]))
            .unwrap();
        wait_session(&session, || {
            session
                .script_lifecycle()
                .latest
                .is_some_and(|latest| latest.run != waiting)
                && session.script_lifecycle().active.is_none()
        });
        for method in [
            "player.joinMap",
            "player.jumpToCell",
            "combat.cancelAutoAttack",
            "flash.callGameFunction0",
        ] {
            assert_eq!(
                recorded.get(method).copied().unwrap_or(0),
                0,
                "{action}: {recorded:?}"
            );
        }
        let info = logs.messages(tracing::Level::INFO);
        assert!(!info.iter().any(|line| line == "waiting body"), "{action}");
        assert_eq!(
            info.iter().filter(|line| *line == "after body").count(),
            1,
            "{action}"
        );
        assert_eq!(
            logs.messages(tracing::Level::DEBUG)
                .iter()
                .filter(|line| *line == "script worker booting")
                .count(),
            1,
            "{action}"
        );
    }
}

#[test]
fn a_reconnect_restart_waits_for_the_new_connections_account() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        counted_movie().responding(
            "flash.getGameObjectS",
            [
                (json!("\"player-a\""), vec![]),
                (json!("\"player-a\""), vec![]),
                (json!("\"player-b\""), vec![]),
            ],
        ),
    );
    session
        .update_script_options(lucent_protocol::script_options::ScriptOptionsPatch {
            restart_after_reconnect: Some(true),
            safe_start_stop: Some(false),
            ..Default::default()
        })
        .unwrap();
    session.supervise(None).unwrap();
    login(&observer);
    session
        .start_script(loose(&[(
            "restart.js",
            r#"
        import * as script from "lucent/script";
        export default function* () { console.log("restart body"); yield* script.sleep("1 hour"); }
    "#,
        )]))
        .unwrap();
    logs.wait_for(tracing::Level::INFO, "restart body");
    emit(&observer, "onConnection", "OnConnectionLost");
    wait_session(&session, || {
        let status = session.script_lifecycle();
        status.waiting_to_restart && status.active.is_none()
    });
    login(&observer);
    wait_session(&session, || {
        let status = session.script_lifecycle();
        !status.waiting_to_restart && status.active.is_none()
    });
    assert_eq!(
        session.script_lifecycle().latest.unwrap().run,
        lucent_host::lifecycle::RunId(1)
    );
    assert_eq!(
        logs.messages(tracing::Level::INFO)
            .iter()
            .filter(|line| *line == "restart body")
            .count(),
        1
    );
}
