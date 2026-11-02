mod common;
use common::TIMEOUT;
use lucent_host::Supervisor;
use lucent_host::bridge::EventInterests;
use lucent_host::{ScriptLink, Session, bridge::Value, testing};
use lucent_protocol::command::Command;
use serde_json::json;
use std::time::{Duration, Instant};

fn start() -> (lucent_host::Host, Session, ScriptLink) {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(
        &host,
        common::ready_movie()
            .recording("settings.setWalkSpeed")
            .recording("settings.enemyMagnet"),
    );
    become_ready(&link);
    update(&link, json!({"walkSpeed":30,"enemyMagnetEnabled":true}));
    (host, session, link)
}

fn become_ready(link: &ScriptLink) {
    link.call_loader(
        "probe.emit",
        vec![
            Value::String("onConnection".into()),
            Value::String("OnConnection".into()),
        ],
    )
    .recv_timeout(TIMEOUT)
    .unwrap();
    load_facts(link);
}

fn load_facts(link: &ScriptLink) {
    for packet in [
        json!({"cmd":"loadInventoryBig","items":[],"hitems":[]}),
        json!({"cmd":"moveToArea","areaId":1,"areaName":"battleon-1",
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}),
    ] {
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
}

fn update(link: &ScriptLink, patch: serde_json::Value) {
    let command: Command =
        serde_json::from_value(json!({"kind":"updateSettings","patch":patch})).unwrap();
    link.command(command).recv_timeout(TIMEOUT).unwrap();
}

fn await_call(link: &ScriptLink) -> lucent_host::bridge::Event {
    use std::{
        sync::Arc,
        task::{Wake, Waker},
        thread,
    };
    struct Unpark(thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    link.set_waker(Waker::from(Arc::new(Unpark(thread::current()))));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match link.try_event().unwrap() {
            Some(lucent_host::bridge::ScriptEvent::Loader { event, .. })
                if event.name == "onProbeCall" =>
            {
                return event;
            }
            Some(_) => {}
            None => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(!remaining.is_zero(), "did not receive a settings call");
                thread::park_timeout(remaining);
            }
        }
    }
}

fn next_call(link: &ScriptLink, method: &str) -> Vec<Value> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        assert!(Instant::now() < deadline, "did not receive {method}");
        let event = await_call(link);
        if event.args.first() == Some(&Value::String(method.into())) {
            return event.args;
        }
    }
}

fn drain(link: &ScriptLink) {
    while link.try_event().unwrap().is_some() {}
}

#[test]
fn the_settings_policy_reapplies_through_the_host() {
    let (_host, mut session, observer) = start();
    drain(&observer);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    assert_eq!(
        next_call(&observer, "settings.setWalkSpeed")[1],
        Value::Number(30.0)
    );
    update(&observer, json!({"walkSpeed":45}));
    drain(&observer);
    let began = Instant::now();
    assert_eq!(
        await_call(&observer).args,
        vec![
            Value::String("settings.enemyMagnet".into()),
            Value::Undefined
        ]
    );
    assert_eq!(
        next_call(&observer, "settings.setWalkSpeed")[1],
        Value::Number(45.0)
    );
    assert!(began.elapsed() < Duration::from_millis(1500));
    update(&observer, json!({"enemyMagnetEnabled":false}));
    drain(&observer);
    let until = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < until {
        let event = await_call(&observer);
        assert_ne!(
            event.args.first(),
            Some(&Value::String("settings.enemyMagnet".into()))
        );
    }
}

#[test]
fn the_native_policy_runs_without_a_bot() {
    use std::{
        sync::Arc,
        task::{Wake, Waker},
        thread,
    };
    struct Unpark(thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let (_host, _session, observer) = start();
    drain(&observer);
    let mut policy = lucent_host::automation::SettingsPolicy::new(
        &observer,
        Waker::from(Arc::new(Unpark(thread::current()))),
    );
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let next = policy.advance(Instant::now());
        while let Some(event) = observer.try_event().unwrap() {
            if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event
                && event.name == "onProbeCall"
                && event.args.first() == Some(&Value::String("settings.setWalkSpeed".into()))
            {
                assert_eq!(event.args[1], Value::Number(30.0));
                return;
            }
        }
        assert!(Instant::now() < deadline);
        thread::park_timeout(
            next.unwrap_or(deadline)
                .min(deadline)
                .saturating_duration_since(Instant::now()),
        );
    }
}

#[test]
fn a_player_restart_rebinds_the_native_policy_to_the_host_settings() {
    let (_host, mut session, observer) = start();
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    next_call(&observer, "settings.setWalkSpeed");
    supervisor
        .restart_player(&mut session, testing::headless_surface())
        .unwrap();
    assert_eq!(
        observer
            .command(Command::ReapplySettings {
                actions_only: false
            })
            .recv_timeout(TIMEOUT),
        Err(lucent_host::bridge::Error::Stopped)
    );
    let observer = session.attach(EventInterests::ALL).unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    testing::wait_loaded(&observer);
    become_ready(&observer);
    drain(&observer);
    assert_eq!(
        next_call(&observer, "settings.setWalkSpeed")[1],
        Value::Number(30.0)
    );
}

#[test]
fn walk_speed_resends_never_go_backwards() {
    let (_host, mut session, observer) = start();
    drain(&observer);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    next_call(&observer, "settings.setWalkSpeed");
    let writer = session.attach(EventInterests::ALL).unwrap();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            for speed in 31..=130 {
                update(&writer, json!({"walkSpeed":speed}));
                drain(&writer);
                if speed % 25 == 0 {
                    next_call(&writer, "settings.enemyMagnet");
                }
            }
        });
        let mut previous = 30.0;
        let mut resends = 0;
        while previous < 130.0 || resends == 0 {
            let call = next_call(&observer, "settings.setWalkSpeed");
            let Value::Number(speed) = call[1] else {
                panic!("missing walk speed")
            };
            assert!(
                speed >= previous,
                "walk speed went backwards from {previous} to {speed}"
            );
            resends += usize::from(speed == previous);
            previous = speed;
        }
        assert_eq!(previous, 130.0);
    });
}

struct Gate {
    message: &'static str,
    entered: std::sync::mpsc::Sender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    remaining: std::sync::atomic::AtomicUsize,
}
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Gate {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        if message.0 == self.message
            && std::thread::current()
                .name()
                .is_some_and(|name| !name.starts_with("lucent-js-watchdog"))
            && self
                .remaining
                .fetch_update(
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                    |n| n.checked_sub(1),
                )
                .is_ok()
        {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
        }
    }
}
fn gate(message: &'static str) -> (Gate, std::sync::mpsc::Receiver<()>, Release) {
    let (entered, ready) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    (
        Gate {
            message,
            entered,
            release: std::sync::Mutex::new(resume),
            remaining: std::sync::atomic::AtomicUsize::new(1),
        },
        ready,
        Release(std::sync::Mutex::new(Some(release))),
    )
}

struct Release(std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>);
impl Release {
    fn send(&self, value: ()) -> Result<(), std::sync::mpsc::SendError<()>> {
        match self.0.lock().unwrap().take() {
            Some(sender) => sender.send(value),
            None => Ok(()),
        }
    }
}
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.send(());
    }
}

#[test]
fn stop_kill_and_status_are_handled_during_boot() {
    use tracing_subscriber::prelude::*;
    for kill in [false, true] {
        let logs = common::logs::Logs::default();
        let (boot, entered, release) = gate("script worker booting");
        let dispatch =
            tracing::Dispatch::new(tracing_subscriber::registry().with(logs.clone()).with(boot));
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let (_host, mut session, observer) = start();
        let mut supervisor = Supervisor::start(&mut session, None).unwrap();
        let controls = supervisor
            .start_script(
                &mut session,
                common::loose(&[("wait.js", "export default function* () {}")]),
            )
            .unwrap();
        entered.recv_timeout(TIMEOUT).unwrap();
        controls.status();
        logs.wait_for(tracing::Level::INFO, "session 1: wait.js is booting");
        if kill {
            controls.kill();
        } else {
            controls.stop();
        }
        logs.wait_for(
            tracing::Level::INFO,
            if kill {
                "session 1: killing wait.js"
            } else {
                "session 1: stopping wait.js"
            },
        );
        release.send(()).unwrap();
        logs.wait_for(
            if kill {
                tracing::Level::WARN
            } else {
                tracing::Level::INFO
            },
            if kill {
                "session 1: wait.js was killed"
            } else {
                "session 1: wait.js was stopped"
            },
        );
        logs.wait_for(tracing::Level::DEBUG, "session 1: script worker reaped");
        controls.status();
        logs.wait_for(tracing::Level::INFO, "session 1: no script is running");
        drain(&observer);
    }
}

#[test]
fn settings_keep_their_cadence_through_boot_watchdog_and_reaping() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        task::{Wake, Waker},
        thread,
    };
    use tracing_subscriber::prelude::*;
    struct Unpark(thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let logs = common::logs::Logs::default();
    let (boot, booted, boot_release) = gate("script worker booting");
    let (reap, reaping, reap_release) = gate("script worker joining");
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::registry()
            .with(logs.clone())
            .with(boot)
            .with(reap),
    );
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let (_host, mut session, observer) = start();
    drain(&observer);
    let stop = Arc::new(AtomicBool::new(false));
    let observer_stop = stop.clone();
    let (calls, next) = std::sync::mpsc::channel();
    let recorder = thread::spawn(move || {
        observer.set_waker(Waker::from(Arc::new(Unpark(thread::current()))));
        let mut times = [Vec::new(), Vec::new()];
        loop {
            while let Some(event) = observer.try_event().unwrap() {
                if let lucent_host::bridge::ScriptEvent::Loader { event, .. } = event
                    && event.name == "onProbeCall"
                {
                    let index = match event.args.first() {
                        Some(Value::String(method)) if method == "settings.setWalkSpeed" => Some(0),
                        Some(Value::String(method)) if method == "settings.enemyMagnet" => Some(1),
                        _ => None,
                    };
                    if let Some(index) = index {
                        let now = Instant::now();
                        times[index].push(now);
                        calls.send((index, now)).unwrap();
                    }
                }
            }
            if observer_stop.load(Ordering::Acquire) {
                return times;
            }
            thread::park();
        }
    });
    let wait_ticks = |count: usize| {
        let began = Instant::now();
        let mut ticks = 0;
        while ticks < count {
            let (index, at) = next.recv_timeout(TIMEOUT).unwrap();
            if index == 0 && at >= began {
                ticks += 1;
            }
        }
    };
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    wait_ticks(2);
    let controls = supervisor
        .start_script(
            &mut session,
            common::loose(&[("spin.js", "export default function* () { while (true) {} }")]),
        )
        .unwrap();
    booted.recv_timeout(TIMEOUT).unwrap();
    controls.status();
    logs.wait_for(tracing::Level::INFO, "session 1: spin.js is booting");
    wait_ticks(3);
    boot_release.send(()).unwrap();
    logs.wait_for(
        tracing::Level::WARN,
        "session 1: spin.js was killed: a turn used more than 200 ms of CPU time",
    );
    reaping.recv_timeout(TIMEOUT).unwrap();
    let busy = supervisor.start_script(
        &mut session,
        common::loose(&[("next.js", "export default function* () {}")]),
    );
    assert!(matches!(
        busy,
        Err(lucent_host::scripting::RuntimeError::Busy)
    ));
    wait_ticks(3);
    assert!(
        !logs
            .messages(tracing::Level::INFO)
            .iter()
            .any(|line| line == "session 1: next.js started")
    );
    reap_release.send(()).unwrap();
    logs.wait_for(tracing::Level::DEBUG, "session 1: script worker reaped");
    supervisor
        .start_script(
            &mut session,
            common::loose(&[("next.js", "export default function* () {}")]),
        )
        .unwrap();
    logs.wait_for(tracing::Level::INFO, "session 1: next.js completed");
    logs.wait_for_count(tracing::Level::DEBUG, "session 1: script worker reaped", 2);
    wait_ticks(2);
    stop.store(true, Ordering::Release);
    recorder.thread().unpark();
    let times = recorder.join().unwrap();
    for (index, bound) in [
        (0, Duration::from_millis(1500)),
        (1, Duration::from_secs(1)),
    ] {
        let gaps: Vec<_> = times[index]
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect();
        assert!(gaps.len() >= 8, "not enough observations: {gaps:?}");
        assert!(
            gaps.iter().sum::<Duration>() / gaps.len() as u32 <= bound,
            "loop {index} average cadence exceeded {bound:?}: {gaps:?}"
        );
        println!(
            "settings loop {index} maximum gap {:?}",
            gaps.iter().max().unwrap()
        );
    }
}

#[test]
fn script_kills_leave_the_same_native_settings_policy_running() {
    let logs = common::logs::Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let (_host, mut session, observer) = start();
    drain(&observer);
    let mut supervisor = Supervisor::start(&mut session, None).unwrap();
    assert_eq!(
        next_call(&observer, "settings.setWalkSpeed")[1],
        Value::Number(30.0)
    );
    for index in 1..=3 {
        supervisor
            .start_script(
                &mut session,
                common::loose(&[("loop.js", "export default function* () { while (true) {} }")]),
            )
            .unwrap();
        logs.wait_for_count(
            tracing::Level::WARN,
            "session 1: loop.js was killed: a turn used more than 200 ms of CPU time",
            index,
        );
        logs.wait_for_count(
            tracing::Level::DEBUG,
            "session 1: script worker reaped",
            index,
        );
        drain(&observer);
        assert_eq!(
            next_call(&observer, "settings.setWalkSpeed")[1],
            Value::Number(30.0)
        );
    }
    assert_eq!(
        logs.messages(tracing::Level::INFO)
            .iter()
            .filter(|line| line.ends_with("supervisor started"))
            .count(),
        1
    );
}

#[test]
fn the_settings_policy_uses_an_existing_link_without_an_event_attachment() {
    let (_host, session, observer) = start();
    let mut policy =
        lucent_host::automation::SettingsPolicy::new(&observer, std::task::Waker::noop().clone());
    assert_eq!(
        session.attached_link_count().recv_timeout(TIMEOUT).unwrap(),
        1
    );
    drop(session);
    assert_eq!(policy.advance(Instant::now()), None);
}

#[test]
fn settings_reapplies_share_one_avatar_confirmation() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        common::ready_movie()
            .recording("auth.isLoggedIn")
            .recording("world.isLoaded")
            .recording("player.isLoaded")
            .recording("settings.setWalkSpeed"),
    );
    become_ready(&observer);
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    let mut calls = [0; 3];
    let mut reapplies = 0;
    while reapplies < 4 {
        let event = await_call(&observer);
        if let Some(Value::String(method)) = event.args.first() {
            match method.as_str() {
                "auth.isLoggedIn" => calls[0] += 1,
                "world.isLoaded" => calls[1] += 1,
                "player.isLoaded" => calls[2] += 1,
                "settings.setWalkSpeed" => reapplies += 1,
                _ => {}
            }
        }
    }
    println!(
        "readiness calls across {reapplies} settings reapplies: auth={}, world={}, player={}",
        calls[0], calls[1], calls[2]
    );
    assert_eq!(calls[0], 0);
    assert_eq!(calls[1], 0);
    assert!(calls[2] <= 1, "avatar confirmations: {}", calls[2]);
}

#[test]
fn settings_wakes_when_avatar_confirmation_finishes() {
    let host = testing::headless_host();
    let (mut session, observer) = testing::start_probe(
        &host,
        common::ready_movie()
            .responding(
                "player.isLoaded",
                [false, false, false, true].map(|loaded| (json!(loaded), vec![])),
            )
            .recording("auth.isLoggedIn")
            .recording("world.isLoaded")
            .recording("settings.setWalkSpeed"),
    );
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
    observer
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    load_facts(&observer);
    let mut confirmations = 0;
    let mut applied = 0;
    while applied < 2 {
        let event = await_call(&observer);
        if let Some(Value::String(method)) = event.args.first() {
            match method.as_str() {
                "auth.isLoggedIn" | "world.isLoaded" => {
                    panic!("unexpected readiness poll: {method}")
                }
                "player.isLoaded" => confirmations += 1,
                "settings.setWalkSpeed" => {
                    assert_eq!(confirmations, 4);
                    applied += 1;
                }
                _ => {}
            }
        }
    }
}
