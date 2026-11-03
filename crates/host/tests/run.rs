mod common;

use common::TIMEOUT;
use lucent_host::scripting::Runtime;
use lucent_host::testing::{self, ProbeMovie};
use lucent_protocol::script::ScriptOutcome;

#[test]
fn a_failed_script_reports_its_error_and_the_next_run_completes() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(&host, common::ready_movie());
    let runtime = Runtime::start(link).unwrap();
    common::ready(&runtime);
    match common::start(
        &mut session,
        common::loose(&[(
            "run.js",
            "export default function* () { throw new Error(\"broken program\"); }",
        )]),
        Default::default(),
    )
    .wait(TIMEOUT)
    .unwrap()
    {
        ScriptOutcome::Failed { message, .. } => assert_eq!(message, "run.js:1:37: broken program"),
        other => panic!("{other:?}"),
    }
    common::completed(&mut session);
}

#[test]
fn a_runtime_owns_no_loader_callbacks_until_its_script_starts() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let runtime = Runtime::start(link).unwrap();

    let callbacks: Vec<String> = runtime
        .eval(
            "first.js",
            r#"["onConnection", "onDebug", "onExtensionResponse", "onProjectionEvent", "packetFromClient", "packetFromServer"]
                .map(name => typeof window[name])"#,
        )
        .wait(TIMEOUT)
        .unwrap();

    assert_eq!(callbacks, ["undefined"; 6]);
}

#[test]
#[ignore = "runs one thousand fresh isolates under externally supplied load"]
fn a_thousand_trivial_runs_complete_without_watchdog_kills() {
    let (_host, mut session, _setup) = common::ready_runtime();
    for index in 1..=1000 {
        common::completed(&mut session);
        if index % 100 == 0 {
            eprintln!("trivial runs completed={index} watchdog_kills=0");
        }
    }
}
