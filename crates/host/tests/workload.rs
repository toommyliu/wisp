mod common;

use common::*;
use lucent_host::Supervisor;
use lucent_host::bridge::EventInterests;
use lucent_host::scripting::Kill;
use lucent_host::{ScriptLink, Session, bridge::ScriptEvent, bridge::Value, testing};
use lucent_protocol::{command::Command, script::Script};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::task::{Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

const SETTINGS: [&str; 2] = ["settings.setWalkSpeed", "settings.enemyMagnet"];
const BATCHES: usize = 20;
const CALL_BATCH: usize = 2500;
const READ_BATCH: usize = 10000;

#[derive(Serialize)]
struct Summary {
    n: usize,
    p50: f64,
    p99: f64,
    max: f64,
}

fn summary(mut values: Vec<f64>) -> Summary {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    assert!(n > 0);
    Summary {
        n,
        p50: values[n.div_ceil(2) - 1],
        p99: values[(n * 99).div_ceil(100) - 1],
        max: values[n - 1],
    }
}

#[derive(Serialize)]
struct Phase {
    name: &'static str,
    wall_ms: f64,
    cpu_ms: Option<f64>,
    rss_mib: Option<f64>,
    walk_speed_gap_ms: Option<Summary>,
    enemy_magnet_gap_ms: Option<Summary>,
    #[serde(skip)]
    window: (Instant, Instant),
}

fn phase(name: &'static str, work: impl FnOnce()) -> Phase {
    let cpu = usage();
    let begin = Instant::now();
    work();
    let end = Instant::now();
    Phase {
        name,
        wall_ms: ms(end - begin),
        cpu_ms: cpu.zip(usage()).map(|((start, _), (end, _))| end - start),
        rss_mib: rss_mib(),
        walk_speed_gap_ms: None,
        enemy_magnet_gap_ms: None,
        window: (begin, end),
    }
}

struct Unpark(thread::Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn observe(link: ScriptLink, stop: Arc<AtomicBool>) -> [Vec<Instant>; 2] {
    link.set_waker(Waker::from(Arc::new(Unpark(thread::current()))));
    let mut calls = [Vec::new(), Vec::new()];
    loop {
        while let Some(event) = link.try_event().unwrap() {
            if let ScriptEvent::Loader { event, .. } = event
                && event.name == "onProbeCall"
                && let Some(Value::String(method)) = event.args.first()
                && let Some(index) = SETTINGS.iter().position(|name| name == method)
            {
                calls[index].push(Instant::now());
            }
        }
        if stop.load(Ordering::Acquire) {
            return calls;
        }
        thread::park();
    }
}

fn gaps(calls: &[Instant], (begin, end): (Instant, Instant)) -> Summary {
    summary(
        calls
            .windows(2)
            .filter(|pair| pair[1] >= begin && pair[1] < end)
            .map(|pair| ms(pair[1] - pair[0]))
            .collect(),
    )
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn started() -> Vec<Value> {
    vec![Value::String("started".into())]
}

fn kill_run(
    session: &mut Session,
    link: &ScriptLink,
    script: &Script,
    kill: Kill,
    supervisor: &mut Supervisor,
    logs: &logs::Logs,
    count: usize,
) -> (f64, f64) {
    let controls = supervisor.start_script(session, script.clone()).unwrap();
    assert_eq!(await_event(link, "onDebug", None).args, started());
    let began = Instant::now();
    if kill == Kill::Requested {
        controls.kill();
    }
    let line = if kill == Kill::Requested {
        "session 1: sleep.js was killed"
    } else {
        "session 1: spin.js was killed: a turn used more than 200 ms of CPU time"
    };
    logs.wait_for_count(tracing::Level::WARN, line, (count - 101) % 10 + 1);
    let terminal = ms(began.elapsed());
    let began = Instant::now();
    logs.wait_for_count(
        tracing::Level::DEBUG,
        "session 1: script worker reaped",
        count,
    );
    (terminal, ms(began.elapsed()))
}

fn kill_summaries(kills: Vec<(f64, f64)>) -> serde_json::Value {
    let (terminal, dropped): (Vec<_>, Vec<_>) = kills.into_iter().unzip();
    json!({"terminal_ms": summary(terminal), "reap_wait_ms": summary(dropped)})
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn usage() -> Option<(f64, f64)> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    assert_eq!(result, 0);
    let usage = unsafe { usage.assume_init() };
    let cpu = (usage.ru_utime.tv_sec as f64 + usage.ru_stime.tv_sec as f64) * 1000.0
        + (usage.ru_utime.tv_usec as f64 + usage.ru_stime.tv_usec as f64) / 1000.0;
    let peak = usage.ru_maxrss as f64 / 1048576.0;
    #[cfg(target_os = "linux")]
    let peak = peak * 1024.0;
    Some((cpu, peak))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn usage() -> Option<(f64, f64)> {
    None
}

#[cfg(target_os = "macos")]
fn rss_mib() -> Option<f64> {
    let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::uninit();
    const PROC_PIDTASKINFO: i32 = 4;
    let size = std::mem::size_of::<libc::proc_taskinfo>() as i32;
    let data = info.as_mut_ptr().cast();
    let pid = unsafe { libc::getpid() };
    let result = unsafe { libc::proc_pidinfo(pid, PROC_PIDTASKINFO, 0, data, size) };
    assert_eq!(result, size);
    Some(unsafe { info.assume_init() }.pti_resident_size as f64 / 1048576.0)
}

#[cfg(target_os = "linux")]
fn rss_mib() -> Option<f64> {
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap();
    let pages: f64 = statm.split_whitespace().nth(1).unwrap().parse().unwrap();
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    assert!(page_size > 0);
    Some(pages * page_size as f64 / 1048576.0)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn rss_mib() -> Option<f64> {
    None
}

#[test]
#[ignore = "measures the real headless runtime; run with pnpm measure:runtime"]
fn runtime_workload() {
    let logs = logs::Logs::default();
    tracing::dispatcher::with_default(&logs.dispatch(), || {
        let host = testing::headless_host();
        let (mut session, link) = testing::start_probe(
            &host,
            SETTINGS.into_iter().fold(
                ready_movie()
                    .combat_facts()
                    .constant("world.getAvailableMonsterMapIds", json!([2]))
                    .constant("combat.attackMonster", json!(true))
                    .constant("combat.useSkill", json!(true)),
                |movie, name| movie.recording(name),
            ),
        );
        log_in(&link);
        let command: Command = serde_json::from_value(json!({"kind":"updateSettings",
            "patch":{"walkSpeed":30,"enemyMagnetEnabled":true}}))
        .unwrap();
        link.command(command).recv_timeout(TIMEOUT).unwrap();
        while link.try_event().unwrap().is_some() {}
        let tick_link = session.attach(EventInterests::ALL).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let observer_stop = stop.clone();
        let observer = thread::spawn(move || observe(tick_link, observer_stop));
        let mut supervisor = Supervisor::start(&mut session, None).unwrap();
        while await_event(&link, "onProbeCall", None).args.first()
            != Some(&Value::String(SETTINGS[0].into()))
        {}

        logs.wait_for(tracing::Level::INFO, "session 1: supervisor started");
        let mut phases = vec![phase("idle", || thread::sleep(Duration::from_secs(10)))];

        let trivial = loose(&[(
            "run.js",
            r#"import * as api from "lucent/api";
            export default function* () { yield* api.packet.sendToClient("started"); }"#,
        )]);
        let mut start_blocked = Vec::new();
        let mut first_action = Vec::new();
        let mut outcome = Vec::new();
        let mut drop_blocked = Vec::new();
        phases.push(phase("runs", || {
            for index in 1..=100 {
                let began = Instant::now();
                supervisor
                    .start_script(&mut session, trivial.clone())
                    .unwrap();
                start_blocked.push(ms(began.elapsed()));
                assert_eq!(await_event(&link, "onDebug", None).args, started());
                first_action.push(ms(began.elapsed()));
                logs.wait_for_count(tracing::Level::INFO, "session 1: run.js completed", index);
                outcome.push(ms(began.elapsed()));
                let began = Instant::now();
                logs.wait_for_count(
                    tracing::Level::DEBUG,
                    "session 1: script worker reaped",
                    index,
                );
                drop_blocked.push(ms(began.elapsed()));
            }
        }));

        let spin = loose(&[(
            "spin.js",
            r#"import * as api from "lucent/api";
            export default function* () { yield* api.packet.sendToClient("started"); while (true) {} }"#,
        )]);
        let sleep = loose(&[(
            "sleep.js",
            r#"import * as api from "lucent/api";
            import * as script from "lucent/script";
            export default function* () {
                yield* api.packet.sendToClient("started");
                yield* script.sleep("1 hour");
            }"#,
        )]);
        let mut watchdog = Vec::new();
        let mut explicit = Vec::new();
        phases.push(phase("kills", || {
            watchdog = (101..=110)
                .map(|count| {
                    kill_run(
                        &mut session,
                        &link,
                        &spin,
                        Kill::Watchdog,
                        &mut supervisor,
                        &logs,
                        count,
                    )
                })
                .collect();
            explicit = (111..=120)
                .map(|count| {
                    kill_run(
                        &mut session,
                        &link,
                        &sleep,
                        Kill::Requested,
                        &mut supervisor,
                        &logs,
                        count,
                    )
                })
                .collect();
        }));

        let measure = loose(&[(
            "latency.js",
            &format!(
                r#"import * as api from "lucent/api";
                export default function* () {{
                    const calls = [], reads = [];
                    for (let batch = 0; batch < {BATCHES}; batch++) {{
                        let began = Date.now();
                        for (let i = 0; i < {CALL_BATCH}; i++) if (!(yield* api.map.isLoaded())) throw new Error("map not loaded");
                        calls.push((Date.now() - began) * 1000 / {CALL_BATCH});
                        began = Date.now();
                        for (let i = 0; i < {READ_BATCH}; i++) if ((yield* api.player.getHp()) !== 100) throw new Error("wrong hp");
                        reads.push((Date.now() - began) * 1000 / {READ_BATCH});
                    }}
                    console.log(JSON.stringify({{ calls, reads }}));
                }}"#
            ),
        )]);
        phases.push(phase("latency", || {
            supervisor.start_script(&mut session, measure).unwrap();
            logs.wait_for(tracing::Level::INFO, "session 1: latency.js completed");
            logs.wait_for_count(
                tracing::Level::DEBUG,
                "session 1: script worker reaped",
                121,
            );
        }));
        #[derive(Deserialize)]
        struct Latency {
            calls: Vec<f64>,
            reads: Vec<f64>,
        }
        let latency = logs
            .messages(tracing::Level::INFO)
            .iter()
            .find_map(|message| serde_json::from_str::<Latency>(message).ok())
            .unwrap();
        assert_eq!(latency.calls.len(), BATCHES);
        assert_eq!(latency.reads.len(), BATCHES);

        let observations = Arc::new(std::sync::Mutex::new(Vec::new()));
        let samples = observations.clone();
        session.set_profile_observation_probe(move |elapsed| {
            samples
                .lock()
                .unwrap()
                .push(elapsed.as_secs_f64() * 1_000_000.0)
        });
        let cycles = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let attacks = cycles.clone();
        session.set_profile_dispatch_probe(move |_, attack| {
            if attack {
                attacks.fetch_add(1, Ordering::Relaxed);
            }
        });
        link.call_loader("probe.emit", vec![Value::String("onExtensionResponse".into()), Value::String(json!({"type":"json","dataObj":{
            "cmd":"moveToArea","areaId":2,"areaName":"test-1",
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100,"intState":1}],
            "monBranch":[{"MonID":1,"MonMapID":2,"strMonName":"Boss","intHP":50,"intHPMax":50,"intState":1}]
        }}).to_string())]).recv_timeout(TIMEOUT).unwrap();
        phases.push(phase("profile", || {
            supervisor.start_script(&mut session, loose(&[("profile.js", r#"
                import { Effect } from "effect";
                import * as api from "lucent/api";
                export default function* () {
                    yield* Effect.raceFirst(api.combat.kill("Boss", {profile:{delayMs:50,steps:[{skill:1}]}}), Effect.sleep("10 seconds"));
                }
            "#)])).unwrap();
            logs.wait_for(tracing::Level::INFO, "session 1: profile.js completed");
            logs.wait_for_count(tracing::Level::DEBUG, "session 1: script worker reaped", 122);
        }));
        let observation_us = observations.lock().unwrap().clone();
        let cycles = cycles.load(Ordering::Relaxed);
        assert!(cycles >= 20, "profile did not fight: {cycles}");
        let observations_per_cycle = observation_us.len() as f64 / cycles as f64;

        phases.push(phase("settle", || thread::sleep(Duration::from_secs(5))));
        stop.store(true, Ordering::Release);
        observer.thread().unpark();
        let [walk_speed, enemy_magnet] = observer.join().unwrap();
        for phase in &mut phases {
            phase.walk_speed_gap_ms = Some(gaps(&walk_speed, phase.window));
            phase.enemy_magnet_gap_ms = Some(gaps(&enemy_magnet, phase.window));
        }
        println!(
            "{}",
            json!({
                "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
                "cpus": thread::available_parallelism().unwrap().get(), "phases": phases,
                "runs": {"start_blocked_ms": summary(start_blocked), "first_action_ms": summary(first_action),
                    "outcome_ms": summary(outcome), "reap_wait_ms": summary(drop_blocked)},
                "watchdog_kills": kill_summaries(watchdog),
                "explicit_kills": kill_summaries(explicit),
                "latency": {"call_us": summary(latency.calls), "read_us": summary(latency.reads),
                    "call_batch": CALL_BATCH, "read_batch": READ_BATCH},
                "profile": {"player_observation_us": summary(observation_us), "cycles": cycles, "observations_per_cycle": observations_per_cycle},
                "peak_rss_mib": usage().map(|(_, peak)| peak)
            })
        );
    });
}
