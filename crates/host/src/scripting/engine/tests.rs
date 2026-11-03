use super::*;
use crate::testing::{self, ProbeMovie};
use lucent_protocol::bridge::Value;
use lucent_protocol::script::{ModulePath, ScriptOutcome};
use serde_json::json;

#[test]
fn a_trivial_scripts_first_turn_stays_below_half_the_watchdog_budget() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .constant("flash.getGameObjectS", json!("\"player-a\""))
            .constant("flash.callGameFunction0", json!("true"))
            .constant("player.isLoaded", json!(true))
            .constant("world.isLoaded", json!(true)),
    );
    for packet in [
        json!({"cmd":"loadInventoryBig", "items":[], "hitems":[{"ItemID":2,"sName":"Home","sType":"House","bEquip":1}]}),
        json!({"cmd":"moveToArea", "areaId":1, "areaName":"buyhouse-1", "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intHPMax":100}]}),
    ] {
        link.call_loader(
            "probe.emit",
            vec![
                Value::String("onExtensionResponse".into()),
                Value::String(json!({"type":"json", "dataObj":packet}).to_string()),
            ],
        )
        .recv_timeout(Duration::from_secs(15))
        .unwrap();
    }
    drop(link);
    for sample in 1..=10 {
        let interrupt = Arc::new(Interrupt::new().unwrap());
        let mut engine = Engine::new(
            session.attach(crate::bridge::EventInterests::ALL).unwrap(),
            interrupt,
            std::task::Waker::noop().clone(),
        )
        .unwrap();
        let entry = ModulePath::loose("trivial.js").unwrap();
        let script = Script::new(
            entry.clone(),
            [(
                entry,
                "globalThis.loaded = true; export default function* () {}".into(),
            )]
            .into(),
            Default::default(),
        )
        .unwrap();
        let (reply, outcome) = mpsc::channel();
        let cpu = crate::scripting::cpu_time::ThreadCpu::current().unwrap();
        let began = cpu.elapsed().unwrap();
        engine.start(script, Default::default(), reply);
        engine.turn("[]").unwrap();
        let elapsed = cpu.elapsed().unwrap() - began;
        match engine
            .evaluate("observed.js", "globalThis.loaded === true")
            .unwrap()
        {
            Completion::Value(value) => assert_eq!(value, "true"),
            Completion::Promise(_) => panic!("expected a boolean"),
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        let json = loop {
            if let Ok(json) = outcome.try_recv() {
                break json.unwrap();
            }
            assert!(Instant::now() < deadline, "script did not complete");
            engine.turn("[]").unwrap();
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(
            serde_json::from_str::<ScriptOutcome>(&json).unwrap(),
            ScriptOutcome::Completed
        );
        eprintln!(
            "first_turn sample={sample} cpu_ms={:.3}",
            elapsed.as_secs_f64() * 1000.0
        );
        assert!(elapsed < Duration::from_millis(100), "{elapsed:?}");
    }
}
