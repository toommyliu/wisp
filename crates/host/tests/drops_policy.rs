mod common;

use common::{TIMEOUT, logs::Logs};
use lucent_host::{
    ScriptLink, Supervisor,
    bridge::{ScriptEvent, Value},
    testing::{self, ProbeMovie},
};
use lucent_protocol::{command::Command, item::ItemContainer};
use serde_json::json;
use std::{
    sync::Arc,
    task::{Wake, Waker},
    thread,
    time::{Duration, Instant},
};

fn packet(data: serde_json::Value) -> String {
    json!({"type":"json","dataObj":data}).to_string()
}
fn emit(link: &ScriptLink, name: &str, payload: String) {
    link.call_loader(
        "probe.emit",
        vec![Value::String(name.into()), Value::String(payload)],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
}
fn drops(link: &ScriptLink, items: &[(i64, &str)]) {
    let items: serde_json::Map<_, _> = items
        .iter()
        .map(|(id, name)| {
            (
                id.to_string(),
                json!({
                    "ItemID":id,"sName":name,"sType":"Item","iQty":1
                }),
            )
        })
        .collect();
    emit(
        link,
        "onExtensionResponse",
        packet(json!({"cmd":"dropItem","items":items})),
    );
}
fn edit(link: &ScriptLink, edit: serde_json::Value) {
    link.command(serde_json::from_value(json!({"kind":"updateEnvironment","edit":edit})).unwrap())
        .recv_timeout(TIMEOUT)
        .unwrap();
}
fn reject_else(link: &ScriptLink) {
    edit(
        link,
        json!({"kind":"setItemRules","rules":{"buckets":[],"rejectElse":true}}),
    );
}
fn movie(reject: bool) -> ProbeMovie {
    common::ready_movie()
        .constant("drops.reject", json!(reject))
        .recording("drops.reject")
        .recording("settings.setWalkSpeed")
        .recording("settings.enemyMagnet")
        .accepting_drop(
            Duration::ZERO,
            packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":true,"iQtyNow":1})),
        )
}
struct Unpark(thread::Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn next(link: &ScriptLink) -> ScriptEvent {
    link.set_waker(Waker::from(Arc::new(Unpark(thread::current()))));
    let until = Instant::now() + TIMEOUT;
    loop {
        if let Some(event) = link.try_event().unwrap() {
            return event;
        }
        let left = until.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "no drop policy event");
        thread::park_timeout(left);
    }
}
fn call(link: &ScriptLink, method: &str) -> Vec<Value> {
    let until = Instant::now() + TIMEOUT;
    loop {
        assert!(Instant::now() < until, "did not receive {method}");
        if let ScriptEvent::Loader { event, .. } = next(link)
            && event.name == "onProbeCall"
            && event.args.first() == Some(&Value::String(method.into()))
        {
            return event.args;
        }
    }
}
fn remaining(link: &ScriptLink) -> Vec<i64> {
    link.read(|p| {
        p.items
            .all(ItemContainer::Drop)
            .iter()
            .map(|i| i.data().item_id)
            .collect()
    })
    .unwrap()
}

#[test]
fn one_wake_reconciles_the_whole_pending_list() {
    let logs = Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, movie(true));
    reject_else(&link);
    drops(&link, &[(1, "Old A"), (2, "Old B"), (3, "New C")]);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    let ids: Vec<_> = (0..3)
        .map(|_| call(&link, "drops.reject")[1].clone())
        .collect();
    assert_eq!(
        ids,
        [Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)]
    );
    logs.wait_for(tracing::Level::DEBUG, "environment drops reconciled");
    assert!(remaining(&link).is_empty());
}

#[test]
fn wanted_names_and_buckets_accept_while_reject_else_rejects_others() {
    for bucket in [
        None,
        Some(("ac-member", true, true)),
        Some(("ac-non-member", true, false)),
        Some(("non-ac-member", false, true)),
        Some(("non-ac-non-member", false, false)),
    ] {
        let logs = Logs::default();
        let _guard = tracing::dispatcher::set_default(&logs.dispatch());
        let host = testing::headless_host();
        let (mut session, link) = testing::start_probe(&host, movie(true));
        if let Some((bucket, coins, member)) = bucket {
            edit(
                &link,
                json!({"kind":"setItemRules","rules":{"buckets":[bucket],"rejectElse":true}}),
            );
            emit(
                &link,
                "onExtensionResponse",
                packet(
                    json!({"cmd":"dropItem","items":{"70":{"ItemID":70,"sName":"Bucket","sType":"Item","bCoins":coins,"bUpg":member,"iQty":1}}}),
                ),
            );
        } else {
            reject_else(&link);
            edit(&link, json!({"kind":"addItem","name":"WANTED"}));
            drops(&link, &[(70, "\u{feff} Wanted \u{a0}")]);
        }
        let coins = bucket.is_some_and(|(_, coins, _)| !coins);
        emit(
            &link,
            "onExtensionResponse",
            packet(
                json!({"cmd":"dropItem","items":{"71":{"ItemID":71,"sName":"Unwanted","sType":"Item","bCoins":coins,"iQty":1}}}),
            ),
        );
        let _supervisor = Supervisor::start(&mut session, None).unwrap();
        logs.wait_for(tracing::Level::DEBUG, "environment drops reconciled");
        assert!(remaining(&link).is_empty());
        let mut accepts = 0;
        let mut rejects = vec![];
        while let Some(event) = link.try_event().unwrap() {
            if let ScriptEvent::Loader { event, .. } = event {
                if event.name == "onProbeAccept" {
                    accepts += 1;
                }
                if event.name == "onProbeCall"
                    && event.args[0] == Value::String("drops.reject".into())
                {
                    rejects.push(event.args[1].clone());
                }
            }
        }
        assert_eq!(accepts, 1);
        assert_eq!(rejects, [Value::Number(71.0)]);
    }
}

#[test]
fn reject_retries_back_off_then_warn_once_with_all_failed_items() {
    let logs = Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, movie(false));
    reject_else(&link);
    drops(&link, &[(1, "A"), (2, "B")]);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    let mut times = [vec![], vec![]];
    let mut order = vec![];
    for _ in 0..8 {
        let args = call(&link, "drops.reject");
        let Value::Number(id) = args[1] else {
            panic!("missing id");
        };
        order.push(id as i64);
        times[id as usize - 1].push(Instant::now());
    }
    logs.wait_for(
        tracing::Level::WARN,
        "environment drop rejection retries exhausted: [1, 2]",
    );
    assert_eq!(&order[..2], &[1, 2]);
    for times in times {
        for (pair, delay) in times.windows(2).zip([100, 200, 400]) {
            assert!(pair[1] - pair[0] >= Duration::from_millis(delay - 10));
        }
    }
    assert_eq!(
        logs.messages(tracing::Level::WARN)
            .iter()
            .filter(|s| s.contains("retries exhausted"))
            .count(),
        1
    );
}

#[test]
fn wakes_during_backoff_coalesce_into_one_follow_up_pass() {
    let logs = Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, movie(false));
    common::log_in(&link);
    reject_else(&link);
    drops(&link, &[(1, "A")]);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    call(&link, "drops.reject");
    for id in 2..=4 {
        drops(&link, &[(id, "Later")]);
    }
    logs.wait_for_count(tracing::Level::DEBUG, "environment drops reconciled", 2);
    let mut ids = vec![];
    while let Some(event) = link.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
            && event.args[0] == Value::String("drops.reject".into())
        {
            ids.push(event.args[1].clone());
        }
    }
    assert_eq!(
        ids.iter().filter(|id| **id == Value::Number(1.0)).count(),
        7
    );
    for id in 2..=4 {
        assert_eq!(
            ids.iter()
                .filter(|v| **v == Value::Number(id as f64))
                .count(),
            4
        );
    }
    call(&link, "settings.setWalkSpeed");
    call(&link, "settings.setWalkSpeed");
    assert_eq!(
        logs.messages(tracing::Level::DEBUG)
            .iter()
            .filter(|s| *s == "environment drops reconciled")
            .count(),
        2
    );
}

#[test]
fn turning_drops_off_stops_retries_and_later_drop_actions() {
    let logs = Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, movie(false));
    common::log_in(&link);
    reject_else(&link);
    drops(&link, &[(1, "A")]);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    call(&link, "drops.reject");
    edit(
        &link,
        json!({"kind":"setAutomationEnabled","capability":"drops","enabled":false}),
    );
    drops(&link, &[(70, "B")]);
    edit(&link, json!({"kind":"addItem","name":"B"}));
    logs.wait_for(tracing::Level::DEBUG, "environment drops reconciled");
    call(&link, "settings.setWalkSpeed");
    call(&link, "settings.setWalkSpeed");
    assert_eq!(remaining(&link), [1, 70]);
    while let Some(event) = link.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event {
            assert_ne!(event.name, "onProbeAccept");
            if event.name == "onProbeCall" {
                assert_ne!(event.args[0], Value::String("drops.reject".into()));
            }
        }
    }
    assert!(
        !logs
            .messages(tracing::Level::WARN)
            .iter()
            .any(|s| s.contains("retries exhausted"))
    );
}

#[test]
fn adding_a_registered_name_accepts_an_already_present_drop() {
    let logs = Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, movie(true));
    drops(&link, &[(70, "Wanted")]);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    edit(&link, json!({"kind":"addItem","name":"wanted"}));
    logs.wait_for(tracing::Level::DEBUG, "environment drops reconciled");
    assert!(remaining(&link).is_empty());
}

#[test]
fn reconnect_discards_old_backoff_and_handles_only_new_drops() {
    let logs = Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, movie(false));
    reject_else(&link);
    drops(&link, &[(1, "Old")]);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    call(&link, "drops.reject");
    emit(&link, "onConnection", "OnConnectionLost".into());
    emit(&link, "onConnection", "OnConnection".into());
    drops(&link, &[(2, "New")]);
    logs.wait_for(
        tracing::Level::WARN,
        "environment drop rejection retries exhausted: [2]",
    );
    assert_eq!(remaining(&link), [2]);
    let mut ids = vec![];
    while let Some(event) = link.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
            && event.args[0] == Value::String("drops.reject".into())
        {
            ids.push(event.args[1].clone());
        }
    }
    assert_eq!(ids, vec![Value::Number(2.0); 4]);
}

#[test]
fn concurrent_script_accept_and_policy_issue_one_accept() {
    let logs = Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        movie(true).accepting_drop(
            Duration::from_secs(1),
            packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":true,"iQtyNow":1})),
        ),
    );
    common::log_in(&link);
    drops(&link, &[(70, "Wanted")]);
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    supervisor
        .start_script(
            &mut session,
            common::loose(&[(
                "accept.js",
                r#"
        import * as api from "lucent/api";
        export default function* () { yield* api.drops.accept(70); }
    "#,
            )]),
        )
        .unwrap();
    common::await_event(&link, "onProbeAccept", None);
    edit(&link, json!({"kind":"addItem","name":"Wanted"}));
    logs.wait_for(tracing::Level::INFO, "session 1: accept.js completed");
    logs.wait_for(tracing::Level::DEBUG, "environment drops reconciled");
    while let Some(event) = link.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event {
            assert_ne!(event.name, "onProbeAccept");
        }
    }
    assert!(remaining(&link).is_empty());
}

#[test]
fn settings_keep_their_cadence_during_a_drop_burst() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, movie(false));
    common::log_in(&link);
    link.command(
        serde_json::from_value::<Command>(
            json!({"kind":"updateSettings","patch":{"walkSpeed":30,"enemyMagnetEnabled":true}}),
        )
        .unwrap(),
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
    reject_else(&link);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    let mut times = [vec![], vec![]];
    let mut rejections = 0;
    let until = Instant::now() + TIMEOUT;
    while times[0].len() < 4 || times[1].len() < 4 || rejections < 32 {
        assert!(Instant::now() < until, "drop burst did not reconcile");
        if let ScriptEvent::Loader { event, .. } = next(&link)
            && event.name == "onProbeCall"
        {
            match &event.args[0] {
                Value::String(method) if method == "settings.setWalkSpeed" => {
                    times[0].push(Instant::now());
                    let items: Vec<_> = (1..=32).map(|id| (id, "Burst")).collect();
                    drops(&link, &items);
                }
                Value::String(method) if method == "settings.enemyMagnet" => {
                    times[1].push(Instant::now())
                }
                Value::String(method) if method == "drops.reject" => rejections += 1,
                _ => {}
            }
        }
    }
    for (times, bound) in times
        .iter()
        .zip([Duration::from_millis(1500), Duration::from_secs(1)])
    {
        let gaps: Vec<_> = times.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(
            gaps.iter().sum::<Duration>() / gaps.len() as u32 <= bound,
            "{gaps:?}"
        );
    }
}

#[test]
fn empty_configuration_does_not_wake_and_overflow_rescans_current_drops() {
    struct Counter {
        wakes: std::sync::atomic::AtomicUsize,
        thread: thread::Thread,
    }
    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.wakes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.thread.unpark();
        }
    }
    let host = testing::headless_host();
    let (mut session, writer) = testing::start_probe(&host, movie(true));
    let counter = Arc::new(Counter {
        wakes: std::sync::atomic::AtomicUsize::new(0),
        thread: thread::current(),
    });
    let mut policy = lucent_host::automation::DropPolicy::new(
        session
            .attach(lucent_host::automation::DropPolicy::INTERESTS)
            .unwrap(),
        Waker::from(counter.clone()),
    );
    assert_eq!(policy.advance(Instant::now()), None);
    for _ in 0..1025 {
        drops(&writer, &[(70, "Wanted")]);
    }
    assert_eq!(counter.wakes.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert_eq!(remaining(&writer), [70]);
    edit(&writer, json!({"kind":"addItem","name":"Wanted"}));
    let until = Instant::now() + TIMEOUT;
    loop {
        let next = policy.advance(Instant::now());
        if remaining(&writer).is_empty()
            && counter.wakes.load(std::sync::atomic::Ordering::Relaxed) > 0
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "overflow did not resynchronize drops"
        );
        thread::park_timeout(
            next.unwrap_or(until)
                .min(until)
                .saturating_duration_since(Instant::now()),
        );
    }
    assert!(counter.wakes.load(std::sync::atomic::Ordering::Relaxed) > 0);
}

#[test]
fn failed_accepts_wait_for_a_later_wake_without_retrying() {
    let logs = Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        movie(true).responding("drops.accept", [(json!(false), vec![])]),
    );
    common::log_in(&link);
    edit(&link, json!({"kind":"addItem","name":"Wanted"}));
    drops(&link, &[(70, "Wanted")]);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    logs.wait_for(tracing::Level::DEBUG, "environment drops reconciled");
    let count = || {
        let mut count = 0;
        while let Some(event) = link.try_event().unwrap() {
            if let ScriptEvent::Loader { event, .. } = event
                && event.name == "onProbeCall"
                && event.args[0] == Value::String("drops.accept".into())
            {
                count += 1;
            }
        }
        count
    };
    assert_eq!(count(), 1);
    call(&link, "settings.setWalkSpeed");
    call(&link, "settings.setWalkSpeed");
    assert_eq!(count(), 0);
    assert_eq!(remaining(&link), [70]);
    assert_eq!(
        logs.messages(tracing::Level::DEBUG)
            .iter()
            .filter(|s| *s == "environment drops reconciled")
            .count(),
        1
    );
    drops(&link, &[(71, "Ignored")]);
    logs.wait_for_count(tracing::Level::DEBUG, "environment drops reconciled", 2);
    assert_eq!(count(), 1);
    assert_eq!(remaining(&link), [70, 71]);
}
