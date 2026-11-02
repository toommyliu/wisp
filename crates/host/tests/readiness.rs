mod common;

use common::TIMEOUT;
use lucent_host::{
    ScriptLink,
    bridge::{ScriptEvent, Value},
    testing,
};
use lucent_protocol::{command::Command, query::Query};
use serde_json::json;
use std::time::{Duration, Instant};

fn emit(link: &ScriptLink, name: &str, payload: String) {
    link.call_loader(
        "probe.emit",
        vec![Value::String(name.into()), Value::String(payload)],
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

fn connect(link: &ScriptLink) {
    emit(link, "onConnection", "OnConnection".into());
}

fn area(link: &ScriptLink) {
    emit(
        link,
        "onExtensionResponse",
        json!({"type":"json","dataObj":{
            "cmd":"moveToArea","areaId":1,"areaName":"battleon-1",
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]
        }})
        .to_string(),
    );
}

fn facts(link: &ScriptLink) {
    area(link);
    emit(
        link,
        "onExtensionResponse",
        json!({"type":"json","dataObj":{
            "cmd":"loadInventoryBig","items":[],"hitems":[]
        }})
        .to_string(),
    );
}

fn ready(link: &ScriptLink) -> bool {
    let snapshot: serde_json::Value =
        serde_json::from_str(&link.query(Query::Readiness {}).unwrap()).unwrap();
    snapshot["canAct"].as_bool().unwrap_or(false)
}

fn confirm(link: &ScriptLink) {
    common::await_event(link, "onProbeCall", None);
    barrier(link);
    assert!(ready(link));
}

#[test]
fn joins_require_one_new_avatar_confirmation() {
    let host = testing::headless_host();
    let (_session, link) =
        testing::start_probe(&host, common::ready_movie().recording("player.isLoaded"));
    connect(&link);
    facts(&link);
    confirm(&link);
    while link.try_event().unwrap().is_some() {}
    area(&link);
    let mut confirmations = 0;
    let mut edges = vec![];
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        match link.try_event().unwrap() {
            Some(ScriptEvent::Readiness { can_act, .. }) => {
                edges.push(can_act);
                if can_act {
                    break;
                }
            }
            Some(ScriptEvent::Loader { event, .. }) if event.name == "onProbeCall" => {
                confirmations += 1
            }
            Some(_) => {}
            None => {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                assert!(!remaining.is_zero(), "did not receive readiness");
                std::thread::park_timeout(remaining);
            }
        }
    }
    assert_eq!(edges, [false, true]);
    assert_eq!(confirmations, 1);
    assert!(ready(&link));
}

#[test]
fn connection_loss_and_logout_stop_avatar_confirmation() {
    for logout in [false, true] {
        let host = testing::headless_host();
        let (_session, link) = testing::start_probe(
            &host,
            common::ready_movie().responding("player.isLoaded", [(json!(false), vec![])]),
        );
        connect(&link);
        facts(&link);
        common::await_event(&link, "onProbeCall", None);
        barrier(&link);
        assert_eq!(
            link.read(|p| p.readiness.avatar).unwrap(),
            lucent_host::projection::Avatar::Loading
        );
        if logout {
            link.command(Command::Logout {})
                .recv_timeout(TIMEOUT)
                .unwrap();
        } else {
            emit(&link, "onConnection", "OnConnectionLost".into());
        }
        barrier(&link);
        assert!(!ready(&link));
        assert!(!link.read(|p| p.readiness.connected).unwrap());
    }
}

#[test]
fn a_restarted_player_has_no_inherited_readiness() {
    let host = testing::headless_host();
    let (mut session, link) =
        testing::start_probe(&host, common::ready_movie().recording("player.isLoaded"));
    connect(&link);
    facts(&link);
    confirm(&link);
    session.restart_player(testing::headless_surface()).unwrap();
    let link = session
        .attach(lucent_host::bridge::EventInterests::ALL)
        .unwrap();
    assert!(!ready(&link));
}

#[test]
fn the_first_settings_apply_waits_for_avatar_art() {
    let logs = common::logs::Logs::default();
    use tracing_subscriber::prelude::*;
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(logs.clone()))
        .unwrap();
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .responding(
                "player.isLoaded",
                [json!(null), json!("true"), json!(false), json!(true)]
                    .map(|value| (value, vec![])),
            )
            .recording("settings.setWalkSpeed"),
    );
    connect(&link);
    facts(&link);
    let mut confirmations = 0;
    loop {
        let call = common::await_event(&link, "onProbeCall", None);
        match call.args.first() {
            Some(Value::String(method)) if method == "player.isLoaded" => confirmations += 1,
            Some(Value::String(method)) if method == "settings.setWalkSpeed" => {
                assert_eq!(confirmations, 4);
                break;
            }
            _ => {}
        }
    }
    assert!(ready(&link));
    assert_eq!(
        logs.messages(tracing::Level::WARN)
            .iter()
            .filter(|message| message.starts_with("player readiness confirmation failed"))
            .count(),
        1
    );
    assert_eq!(
        logs.messages(tracing::Level::INFO)
            .iter()
            .filter(|message| message.as_str() == "player readiness confirmation recovered")
            .count(),
        1
    );
}

#[test]
fn event_overflow_discards_avatar_readiness_until_facts_recover() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .recording("player.isLoaded")
            .raising_many("probe.overflow", "onDebug", "overflow", 1025),
    );
    connect(&link);
    facts(&link);
    confirm(&link);
    let before = link.read(|p| p.receipts.epoch()).unwrap();
    link.call_loader("probe.overflow", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    barrier(&link);
    assert!(!ready(&link));
    assert_eq!(link.read(|p| p.receipts.epoch()).unwrap(), before + 1);
    facts(&link);
    confirm(&link);
}

#[test]
fn a_reply_from_before_a_join_cannot_mark_the_avatar_loaded() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        common::ready_movie().responding(
            "player.isLoaded",
            [
                (
                    json!(true),
                    vec![("onExtensionResponse".into(), json!({
                    "type":"json", "dataObj":{
                        "cmd":"moveToArea", "areaId":2, "areaName":"battleon-2",
                        "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]
                    }
                }).to_string())],
                ),
                (json!(false), vec![]),
            ],
        ),
    );
    connect(&link);
    facts(&link);
    let mut confirmations = 0;
    let deadline = Instant::now() + TIMEOUT;
    while confirmations < 2 {
        match next_event(&link, deadline) {
            ScriptEvent::Readiness { can_act, .. } => {
                assert!(!can_act, "stale confirmation made the new map ready")
            }
            ScriptEvent::Loader { event, .. }
                if event.name == "onProbeCall"
                    && event.args.first() == Some(&Value::String("player.isLoaded".into())) =>
            {
                confirmations += 1
            }
            _ => {}
        }
    }
    barrier(&link);
    while let Some(event) = link.try_event().unwrap() {
        if let ScriptEvent::Readiness { can_act, .. } = event {
            assert!(!can_act, "stale confirmation published readiness");
        }
    }
    assert!(!ready(&link));
    assert_eq!(
        link.read(|p| p.readiness.avatar).unwrap(),
        lucent_host::projection::Avatar::Loading
    );
}

fn next_event(link: &ScriptLink, deadline: Instant) -> ScriptEvent {
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
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "no player event before deadline");
        std::thread::park_timeout(remaining);
    }
}

#[test]
fn a_failed_logout_stays_unready_until_the_next_connection() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .throwing("auth.logout", "logout failed")
            .recording("player.isLoaded"),
    );
    connect(&link);
    facts(&link);
    confirm(&link);
    assert!(
        link.command(Command::Logout {})
            .recv_timeout(TIMEOUT)
            .is_err()
    );
    barrier(&link);
    assert!(!ready(&link));
    area(&link);
    assert!(!ready(&link));
    connect(&link);
    facts(&link);
    let deadline = Instant::now() + TIMEOUT;
    while !ready(&link) {
        next_event(&link, deadline);
    }
}

#[test]
fn slow_frames_do_not_delay_avatar_confirmation() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .frame_rate(2.0)
            .responding("player.isLoaded", [(json!(false), vec![])]),
    );
    connect(&link);
    facts(&link);
    let deadline = Instant::now() + TIMEOUT;
    let mut stamps = vec![];
    while stamps.len() < 5 {
        if let ScriptEvent::Loader { event, .. } = next_event(&link, deadline)
            && event.name == "onProbeCall"
            && event.args.first() == Some(&Value::String("player.isLoaded".into()))
        {
            stamps.push(Instant::now());
        }
    }
    let gaps: Vec<_> = stamps.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(
        gaps.iter().all(|gap| *gap < Duration::from_millis(350)),
        "confirmations waited for slow frames: {gaps:?}"
    );
    assert!(
        stamps[4] - stamps[1] >= Duration::from_millis(280),
        "confirmations exceeded the ten-per-second limit: {gaps:?}"
    );
}
