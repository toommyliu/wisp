mod common;
use common::*;
use lucent_host::{Session, testing};
use lucent_protocol::{command::Command, script::ScriptPhase};
use std::{
    sync::{Arc, mpsc},
    task::{Wake, Waker},
};

struct Changed(mpsc::Sender<()>);
impl Wake for Changed {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }
}
fn watch(session: &Session) -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel();
    let waker = Waker::from(Arc::new(Changed(tx)));
    session.watch(waker.clone());
    session.watch(waker);
    rx
}
struct Transition(mpsc::Sender<mpsc::Sender<()>>);
impl Wake for Transition {
    fn wake(self: Arc<Self>) {
        let (tx, rx) = mpsc::channel();
        if self.0.send(tx).is_ok() {
            assert!(!matches!(
                rx.recv_timeout(TIMEOUT),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
        }
    }
}
fn transition(
    session: &Session,
    changes: &mpsc::Receiver<mpsc::Sender<()>>,
    expected: ScriptPhase,
) {
    let release = changes.recv_timeout(TIMEOUT).unwrap();
    let actual = session.script_status().phase;
    release.send(()).unwrap();
    assert_eq!(actual, expected);
}
fn command(session: &Session, value: serde_json::Value) {
    session
        .command(serde_json::from_value::<Command>(value).unwrap())
        .recv_timeout(TIMEOUT)
        .unwrap();
}
#[test]
fn records_notify_only_on_changes_and_survive_player_restart() {
    let host = testing::headless_host();
    let mut session = host.start_session(testing::headless_surface()).unwrap();
    let changes = watch(&session);
    for value in [
        serde_json::json!({"kind":"updateSettings","patch":{"walkSpeed":99,"otherPlayersVisible":false}}),
        serde_json::json!({"kind":"patchAutoZone","patch":{"enabled":true,"map":"ultradage"}}),
    ] {
        command(&session, value.clone());
        assert_eq!(changes.try_iter().count(), 1);
        command(&session, value);
        assert_eq!(changes.try_iter().count(), 0);
    }
    assert_eq!(session.settings().walk_speed, 99.0);
    assert!(!session.settings().other_players_visible);
    assert!(session.auto_zone().enabled);
    session.supervise(None).unwrap();
    command(
        &session,
        serde_json::json!({"kind":"setAutoReloginDelay","delayMs":5000}),
    );
    assert_eq!(session.auto_relogin().config.delay_ms, 5000.0);
    let settings = session.settings();
    let zone = session.auto_zone();
    session.restart_player(testing::headless_surface()).unwrap();
    assert_eq!(session.settings(), settings);
    assert_eq!(session.auto_zone(), zone);
    assert_eq!(session.auto_relogin().config.delay_ms, 5000.0);
    changes.try_iter().for_each(drop);
    command(
        &session,
        serde_json::json!({"kind":"updateSettings","patch":{"walkSpeed":30}}),
    );
    assert_eq!(changes.try_iter().count(), 1);
}
#[test]
fn session_starts_stops_and_kills_scripts() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    session.supervise(None).unwrap();
    let (tx, changes) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Transition(tx))));
    assert_eq!(session.script_status().phase, ScriptPhase::Idle);
    let source = r#"import * as script from 'lucent/script'; export default function* () { yield* script.sleep('1 hour'); }"#;
    session.start_script(loose(&[("wait.js", source)])).unwrap();
    transition(&session, &changes, ScriptPhase::Waiting);
    log_in(&observer);
    transition(&session, &changes, ScriptPhase::Preparing);
    transition(&session, &changes, ScriptPhase::Starting);
    transition(&session, &changes, ScriptPhase::Running);
    session.stop_script();
    transition(&session, &changes, ScriptPhase::Stopping);
    transition(&session, &changes, ScriptPhase::Finishing);
    transition(&session, &changes, ScriptPhase::Idle);
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("wait.js was stopped")
    );
    session.start_script(loose(&[("wait.js", source)])).unwrap();
    transition(&session, &changes, ScriptPhase::Waiting);
    transition(&session, &changes, ScriptPhase::Preparing);
    transition(&session, &changes, ScriptPhase::Starting);
    transition(&session, &changes, ScriptPhase::Running);
    session.kill_script();
    transition(&session, &changes, ScriptPhase::Finishing);
    transition(&session, &changes, ScriptPhase::Idle);
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("wait.js was killed")
    );
    let status = session.script_status();
    session.kill_script();
    assert_eq!(session.script_status(), status);
    session
        .start_script(loose(&[("done.js", "export default function* () {}")]))
        .unwrap();
    transition(&session, &changes, ScriptPhase::Waiting);
    transition(&session, &changes, ScriptPhase::Preparing);
    transition(&session, &changes, ScriptPhase::Starting);
    transition(&session, &changes, ScriptPhase::Running);
    transition(&session, &changes, ScriptPhase::Finishing);
    transition(&session, &changes, ScriptPhase::Idle);
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("done.js completed")
    );
    drop(changes);
}

#[test]
fn shell_and_script_commands_publish_the_same_records_after_loading() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, ready_movie());
    session.supervise(None).unwrap();
    let changes = watch(&session);
    for value in [
        serde_json::json!({"kind":"updateSettings","patch":{"walkSpeed":30}}),
        serde_json::json!({"kind":"patchAutoZone","patch":{"map":"queeniona"}}),
        serde_json::json!({"kind":"setAutoReloginDelay","delayMs":5000}),
    ] {
        let shell = session
            .command(serde_json::from_value(value.clone()).unwrap())
            .recv_timeout(TIMEOUT)
            .unwrap();
        assert_eq!(changes.try_iter().count(), 1, "command {value}");
        let script = link
            .command(serde_json::from_value(value.clone()).unwrap())
            .recv_timeout(TIMEOUT)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&shell).unwrap(),
            serde_json::from_str::<serde_json::Value>(&script).unwrap()
        );
        assert_eq!(changes.try_iter().count(), 0);
        command(&session, value);
        assert_eq!(changes.try_iter().count(), 0);
    }
    assert_eq!(session.settings().walk_speed, 30.0);
    assert_eq!(
        session.auto_zone().map,
        Some(lucent_protocol::auto_zone::AutoZoneMap::Queeniona)
    );
    command(
        &session,
        serde_json::json!({"kind":"setAutoReloginEnabled","enabled":true}),
    );
    assert!(session.auto_relogin().config.enabled);
}

#[test]
fn stopping_a_waiting_script_cancels_it_without_starting_a_runtime() {
    let host = testing::headless_host();
    let (mut session, _) = testing::start_probe(&host, ready_movie());
    session.supervise(None).unwrap();
    let (tx, changes) = mpsc::channel();
    session.watch(Waker::from(Arc::new(Transition(tx))));
    session
        .start_script(loose(&[("wait.js", "export default function* () {}")]))
        .unwrap();
    transition(&session, &changes, ScriptPhase::Waiting);
    session.stop_script();
    transition(&session, &changes, ScriptPhase::Finishing);
    transition(&session, &changes, ScriptPhase::Idle);
    assert_eq!(session.script_status().script, None);
    assert_eq!(
        session.script_status().last_end.as_deref(),
        Some("wait.js was stopped")
    );
    drop(changes);
}

#[test]
fn auto_attack_records_and_watchers_survive_player_replacement() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        ready_movie()
            .combat_facts()
            .constant("world.getAvailableMonsterMapIds", serde_json::json!([]))
            .constant("combat.getConsumableSkillItem", serde_json::Value::Null)
            .constant("combat.cancelAutoAttack", serde_json::Value::Null)
            .constant("combat.cancelTarget", serde_json::Value::Null),
    );
    log_in(&observer);
    session.supervise(None).unwrap();
    let changes = watch(&session);
    command(
        &session,
        serde_json::json!({"kind":"enableAutoAttack","profile":null,"targetPriority":[]}),
    );
    let config = session.auto_attack().config;
    assert!(config.enabled);
    assert_eq!(config.profile.as_ref().unwrap().id, "generic-base");
    assert!(changes.try_iter().count() > 0);
    session.restart_player(testing::headless_surface()).unwrap();
    assert_eq!(session.auto_attack().config, config);
    command(&session, serde_json::json!({"kind":"disableAutoAttack"}));
    assert!(!session.auto_attack().config.enabled);
    assert!(changes.try_iter().count() > 0);
    command(&session, serde_json::json!({"kind":"disableAutoAttack"}));
    assert_eq!(changes.try_iter().count(), 0);
}
