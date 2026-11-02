mod common;

use common::*;
use lucent_host::{
    Supervisor,
    bridge::{EventInterests, ScriptEvent, Value},
    testing,
};
use lucent_protocol::script::ScriptOutcome;
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Wake, Waker},
    time::Instant,
};

const JOINS: usize = 200;

type Calls = std::collections::BTreeMap<String, usize>;
struct Collection {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Calls>>,
}
impl Collection {
    fn finish(&mut self) -> std::thread::Result<Calls> {
        self.stop.store(true, Ordering::Release);
        let thread = self.thread.take().unwrap();
        thread.thread().unpark();
        thread.join()
    }
}
impl Drop for Collection {
    fn drop(&mut self) {
        if self.thread.is_some() {
            let _ = self.finish();
        }
    }
}

struct Unpark(std::thread::Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

#[test]
#[ignore = "measures native and script travel through the headless player"]
fn travel_join_workload() {
    let host = testing::headless_host();
    let replies = (0..JOINS).map(|index| {
        let name = if index % 2 == 0 { "doom" } else { "battleon" };
        (
            json!(null),
            vec![("onExtensionResponse".into(), json!({"type":"json","dataObj": {
            "cmd":"moveToArea","areaId":index + 2,"areaName":format!("{name}-1"),
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}]
        }}).to_string())],
        )
    });
    let movie = ready_movie()
        .constant("world.isActionAvailable", json!(true))
        .responding("player.joinMap", replies)
        .recording("auth.isLoggedIn")
        .recording("world.isLoaded")
        .recording("player.isLoaded")
        .recording("world.isActionAvailable");
    let (mut session, observer) = testing::start_probe(&host, movie);
    log_in(&observer);
    session
        .update_script_options(
            serde_json::from_value(json!({"roomPolicy":{"kind":"public"}})).unwrap(),
        )
        .unwrap();
    let _supervisor = Supervisor::start(&mut session, None).unwrap();
    let completion = session.attach(EventInterests::ALL).unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    while observer.try_event().unwrap().is_some() {}
    let thread = std::thread::spawn(move || {
        observer.set_waker(Waker::from(Arc::new(Unpark(std::thread::current()))));
        let mut calls = std::collections::BTreeMap::<String, usize>::new();
        loop {
            while let Some(event) = observer.try_event().unwrap() {
                if let ScriptEvent::Loader { event, .. } = event
                    && event.name == "onProbeCall"
                    && let Some(Value::String(method)) = event.args.first()
                {
                    *calls.entry(method.clone()).or_default() += 1;
                }
            }
            if stop.load(Ordering::Acquire) {
                break;
            }
            std::thread::park();
        }
        calls
    });
    let mut collection = Collection {
        stop: stopped,
        thread: Some(thread),
    };
    let cpu = session
        .player_cpu_time()
        .recv_timeout(TIMEOUT)
        .unwrap()
        .unwrap();
    let began = Instant::now();
    let run = start(
        &mut session,
        loose(&[(
            "joins.js",
            r#"
        import * as api from "lucent/api";
        export default function* () {
            for (let i = 0; i < 200; i++) {
                if (!(yield* api.player.joinMap(i % 2 === 0 ? "doom" : "battleon"))) throw new Error("join failed " + i);
            }
        }
    "#,
        )]),
        Default::default(),
    );
    assert_eq!(run.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    let wall = began.elapsed();
    let cpu = session
        .player_cpu_time()
        .recv_timeout(TIMEOUT)
        .unwrap()
        .unwrap()
        - cpu;
    completion
        .call_loader("probe.echo", vec![])
        .recv_timeout(TIMEOUT)
        .unwrap();
    let calls = collection.finish().unwrap();
    assert_eq!(calls.get("player.joinMap"), Some(&JOINS));
    println!(
        "TRAVEL_WORKLOAD {}",
        json!({"joins":JOINS,"calls":calls,"player_cpu_us_per_join":cpu.as_secs_f64()*1e6/JOINS as f64,"wall_ms_per_join":wall.as_secs_f64()*1e3/JOINS as f64})
    );
}
