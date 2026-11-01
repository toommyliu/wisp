#![allow(dead_code)]

use lucent_host::bridge::EventInterests;
use lucent_host::scripting::{Runtime, ScriptRun};
use lucent_host::testing::{self, ProbeMovie};
use lucent_host::{Host, Session};
use lucent_protocol::script::{ModulePath, Script};
use serde_json::json;
use std::{collections::BTreeMap, path::Path, time::Duration};

pub const TIMEOUT: Duration = Duration::from_secs(15);

pub fn loose(modules: &[(&str, &str)]) -> Script {
    Script::new(
        ModulePath::loose(modules[0].0).unwrap(),
        modules
            .iter()
            .map(|(path, source)| (ModulePath::loose(*path).unwrap(), (*source).into()))
            .collect(),
        BTreeMap::new(),
    )
    .unwrap()
}

pub fn package(dir: &str, entry: &str, extra: &[(&str, &str)]) -> Script {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../script-packages")
        .join(dir);
    let mut package = lucent_host::library::Package::read(&dir).unwrap();
    for (path, source) in extra {
        package.modules.insert(
            ModulePath::in_package(&package.name, *path).unwrap(),
            (*source).into(),
        );
    }
    package.script(entry).unwrap()
}

pub fn ready_movie() -> ProbeMovie {
    let movie = ProbeMovie::new()
        .raising("flash.sendClientPacket", "onDebug", "started")
        .constant("auth.isLoggedIn", json!(true))
        .constant("flash.getGameObjectS", json!("\"player-a\""))
        .constant("auth.logout", json!(null))
        .constant("flash.callGameFunction0", json!("true"))
        .constant("player.isLoaded", json!(true))
        .constant("world.isLoaded", json!(true))
        .constant("player.walkTo", json!(true))
        .constant("bank.loadItems", json!(null))
        .constant("bank.isLoaded", json!(true))
        .constant("bank.getItems", json!([]))
        .raising(
            "quests.getMultiple",
            "onExtensionResponse",
            json!({
                "type": "json",
                "dataObj": {
                    "cmd": "getQuests",
                    "quests": {
                        "802": {
                            "QuestID": 802,
                            "sName": "Elders Blood",
                            "Rewards": [{"ItemID": 5586, "sName": "Elders Blood", "iStk": 20}],
                        },
                    },
                },
            })
            .to_string(),
        );
    [
        "settings.setAnimationsEnabled",
        "settings.setCollisionsEnabled",
        "settings.setCustomGuild",
        "settings.resetCustomGuild",
        "settings.setCustomName",
        "settings.resetCustomName",
        "settings.setDeathAdsVisible",
        "settings.setOtherPlayersVisible",
        "settings.setWalkSpeed",
        "settings.setLagKillerEnabled",
        "settings.setFrameRate",
        "settings.enemyMagnet",
        "settings.infiniteRange",
        "settings.provokeCell",
        "settings.skipCutscenes",
    ]
    .into_iter()
    .fold(movie, |movie, name| movie.constant(name, json!(null)))
}

pub fn ready(runtime: &Runtime) {
    runtime
        .eval(
            "ready.js",
            r#"
        window.swf["probe.emit"]("onExtensionResponse", JSON.stringify({
            type: "json",
            dataObj: {
                cmd: "loadInventoryBig",
                items: [{ ItemID: 5586, sName: "Elders Blood", sType: "Item", iQty: 20, iStk: 20 }],
                hitems: [{ ItemID: 2, sName: "Home", sType: "House", bEquip: 1 }],
            },
        }));
        window.swf["probe.emit"]("onExtensionResponse", JSON.stringify({
            type: "json",
            dataObj: {
                cmd: "moveToArea",
                areaId: 1,
                areaName: "buyhouse-1",
                uoBranch: [{ entID: 7, uoName: "player-a", intHP: 100, intHPMax: 100 }],
            },
        }));
        null;
    "#,
        )
        .wait::<()>(TIMEOUT)
        .unwrap();
}

pub fn log_in(link: &lucent_host::ScriptLink) {
    use lucent_protocol::bridge::Value;
    for packet in [
        json!({"type":"json","dataObj":{"cmd":"loadInventoryBig",
            "items":[{"ItemID":5586,"sName":"Elders Blood","sType":"Item","iQty":20,"iStk":20}],
            "hitems":[{"ItemID":2,"sName":"Home","sType":"House","bEquip":1}]}}),
        json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":1,"areaName":"buyhouse-1",
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}}),
    ] {
        link.call_loader(
            "probe.emit",
            vec![
                Value::String("onExtensionResponse".into()),
                Value::String(packet.to_string()),
            ],
        )
        .recv_timeout(TIMEOUT)
        .unwrap();
    }
    for command in [
        lucent_protocol::command::Command::RefreshCredentials {},
        lucent_protocol::command::Command::RefreshLoggedIn {},
    ] {
        link.command(command).recv_timeout(TIMEOUT).unwrap();
    }
}

pub fn start(
    session: &mut Session,
    script: Script,
    inputs: lucent_protocol::script::ScriptInputs,
) -> ScriptRun {
    ScriptRun::start(session.attach(EventInterests::ALL).unwrap(), script, inputs).unwrap()
}

pub fn await_event(
    link: &lucent_host::ScriptLink,
    name: &str,
    run: Option<&ScriptRun>,
) -> lucent_protocol::bridge::Event {
    use std::sync::Arc;
    use std::task::{Wake, Waker};
    use std::time::Instant;
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    link.set_waker(Waker::from(Arc::new(Unpark(std::thread::current()))));
    let deadline = Instant::now() + TIMEOUT;
    let mut outcome = None;
    loop {
        outcome = outcome.or_else(|| {
            run.and_then(|run| match run.wait(Duration::ZERO) {
                Err(lucent_host::scripting::RuntimeError::Timeout) => None,
                outcome => Some(outcome),
            })
        });
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "did not receive {name}");
        match link.try_event().unwrap() {
            Some(lucent_host::bridge::ScriptEvent::Loader { event, .. }) if event.name == name => {
                return event;
            }
            Some(_) => {}
            None => {
                if let Some(outcome) = outcome {
                    panic!("run ended before {name}: {outcome:?}");
                }
                std::thread::park_timeout(if run.is_some() {
                    remaining.min(Duration::from_millis(10))
                } else {
                    remaining
                });
            }
        }
    }
}

pub fn started_run(session: &mut Session, source: &str) -> ScriptRun {
    let observer = session.attach(EventInterests::ALL).unwrap();
    let run = start(session, loose(&[("wait.js", source)]), BTreeMap::new());
    assert_eq!(
        await_event(&observer, "onDebug", Some(&run)).args,
        vec![lucent_protocol::bridge::Value::String("started".into())]
    );
    run
}

pub fn yielding(session: &mut Session) -> ScriptRun {
    started_run(
        session,
        r#"
        import * as api from "lucent/api";
        import * as script from "lucent/script";
        export default function* () {
            yield* api.packet.sendToClient("started");
            yield* script.sleep("1 hour");
        }
    "#,
    )
}

pub fn completed(session: &mut Session) {
    assert_eq!(
        start(
            session,
            loose(&[("done.js", "export default function* () {}")]),
            BTreeMap::new()
        )
        .wait(TIMEOUT)
        .unwrap(),
        lucent_protocol::script::ScriptOutcome::Completed
    );
}

pub fn ready_runtime() -> (Host, Session, Runtime) {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(&host, ready_movie());
    let runtime = Runtime::start(link).unwrap();
    ready(&runtime);
    (host, session, runtime)
}

#[cfg(target_os = "macos")]
pub fn js_threads() -> Vec<String> {
    const PROC_PIDTHREADINFO: i32 = 5;
    const PROC_PIDLISTTHREADS: i32 = 6;
    unsafe {
        let pid = libc::getpid();
        let mut ids = [0u64; 512];
        let bytes = libc::proc_pidinfo(
            pid,
            PROC_PIDLISTTHREADS,
            0,
            ids.as_mut_ptr().cast(),
            (ids.len() * 8) as i32,
        );
        let mut names: Vec<String> = ids[..bytes as usize / 8]
            .iter()
            .map(|id| {
                let mut info: libc::proc_threadinfo = std::mem::zeroed();
                let size = std::mem::size_of_val(&info) as i32;
                if libc::proc_pidinfo(
                    pid,
                    PROC_PIDTHREADINFO,
                    *id,
                    (&mut info as *mut libc::proc_threadinfo).cast(),
                    size,
                ) != size
                {
                    return "<exited>".into();
                }
                let name = std::ffi::CStr::from_ptr(info.pth_name.as_ptr())
                    .to_string_lossy()
                    .into_owned();
                if name.is_empty() {
                    "<unnamed>".into()
                } else {
                    name
                }
            })
            .collect();
        names.retain(|name| name.starts_with("lucent-js-"));
        names.sort();
        names
    }
}

pub mod logs;

#[cfg(target_os = "linux")]
pub fn js_threads() -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir("/proc/self/task")
        .unwrap()
        .filter_map(
            |entry| match std::fs::read_to_string(entry.unwrap().path().join("comm")) {
                Ok(name) => Some(name),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => panic!("could not read thread name: {error}"),
            },
        )
        .map(|name| name.trim().to_owned())
        .filter(|name| name.starts_with("lucent-js-"))
        .collect();
    names.sort();
    names
}

pub mod hold;
