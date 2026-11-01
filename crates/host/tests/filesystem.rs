mod common;

use common::*;
use lucent_host::Host;
use lucent_host::scripting::ScriptRun;
use lucent_host::testing;
use lucent_protocol::script::ScriptOutcome;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lucent-fs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("data")).unwrap();
        Self(path)
    }
    fn root(&self) -> PathBuf {
        self.0.join("data")
    }
    fn host(&self) -> Host {
        testing::headless_host_with_data(self.root())
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn run(host: &Host, body: &str) -> Value {
    let (_session, link) = testing::start_probe(host, ready_movie());
    log_in(&link);
    let source = format!(
        r#"
        import * as fs from "lucent/filesystem";
        import * as script from "lucent/script";
        import {{ Effect }} from "effect";
        const match = ({{onFailure, onSuccess}}) => effect => effect.pipe(Effect.map(onSuccess), Effect.catch(error => Effect.succeed(onFailure(error))));
        export default function* () {{
            const result = yield* Effect.gen(function* () {{ {body} }});
            yield* script.stop(JSON.stringify(result));
        }}
    "#
    );
    let run = ScriptRun::start(link, loose(&[("files.js", &source)]), Default::default()).unwrap();
    match run.wait(TIMEOUT).unwrap() {
        ScriptOutcome::StoppedByScript {
            reason: Some(value),
        } => serde_json::from_str(&value).unwrap(),
        outcome => panic!("{outcome:?}"),
    }
}

#[test]
fn text_and_json_round_trip_and_missing_files_are_undefined() {
    let workspace = Workspace::new();
    let host = workspace.host();
    assert_eq!(
        run(
            &host,
            r#"
        yield* fs.writeText("nested/text.txt", "héllo\n");
        yield* fs.writeJson("nested/value.json", { count: 2, names: ["é", null] });
        const result = [yield* fs.readText("nested/text.txt"), yield* fs.readJson("nested/value.json"),
            (yield* fs.readText("missing")) === undefined, (yield* fs.readJson("missing")) === undefined,
            yield* fs.exists("nested/text.txt"), yield* fs.list("nested")];
        yield* fs.remove("nested/text.txt");
        yield* fs.remove("nested/value.json");
        yield* fs.remove("nested");
        yield* fs.remove("missing");
        return [...result, yield* fs.exists("nested")];
    "#
        ),
        json!(["héllo\n", {"count":2,"names":["é",null]}, true, true, true,
        [{"name":"text.txt","kind":"file"},{"name":"value.json","kind":"file"}], false])
    );
}

#[test]
fn unsafe_paths_never_write_outside_the_data_folder() {
    let workspace = Workspace::new();
    let host = workspace.host();
    assert_eq!(
        run(
            &host,
            r#"
        const paths = ["../escape", "/escape", "a\\b", "CON.txt", "a//b", "a/./b", "trailing.",
            "a ", "bad:name", "control\u0001", "e\u0301", "a".repeat(256), Array(33).fill("a").join("/"),
            Array(8).fill("a".repeat(128)).join("/"), ".lucent-atomic-private"];
        return yield* Effect.forEach(paths, path => fs.writeText(path, "escape").pipe(
            match({ onFailure: error => error.reason, onSuccess: () => "wrote" })));
    "#
        ),
        json!(vec!["invalid-path"; 15])
    );
    assert_eq!(fs::read_dir(&workspace.0).unwrap().count(), 1);
    assert_eq!(fs::read_dir(workspace.root()).unwrap().count(), 0);
}

#[test]
fn windows_device_names_are_invalid_for_every_filesystem_operation() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let results = run(
        &host,
        r#"
        const names = ["COM0", "LPT0", "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³", "CONIN$", "CONOUT$"];
        const results = [];
        for (const name of names) {
            const mixed = name[0] + name.slice(1).toLowerCase();
            for (const path of [name, `${name}.txt`, mixed, `${mixed}.TxT`]) {
                for (const effect of [fs.readText(path), fs.readJson(path), fs.writeText(path, "text"),
                    fs.writeJson(path, {}), fs.list(path), fs.exists(path), fs.remove(path)]) {
                    results.push([path, yield* effect.pipe(match({
                        onFailure: e => e.reason, onSuccess: () => "accepted"
                    }))]);
                }
            }
        }
        return results;
    "#,
    );
    for result in results.as_array().unwrap() {
        assert_eq!(result[1], json!("invalid-path"), "{}", result[0]);
    }
    assert_eq!(results.as_array().unwrap().len(), 10 * 4 * 7);
    assert_eq!(fs::read_dir(workspace.root()).unwrap().count(), 0);
}

#[test]
fn reads_and_writes_enforce_the_file_limit() {
    let workspace = Workspace::new();
    fs::write(
        workspace.root().join("large"),
        vec![b'x'; 8 * 1024 * 1024 + 1],
    )
    .unwrap();
    assert_eq!(
        run(
            &workspace.host(),
            r#"
        const read = yield* fs.readText("large").pipe(match({onFailure: e => e.reason, onSuccess: () => "read"}));
        if (read !== "too-large") return [read];
        return [read,
            yield* fs.writeText("oversize", "é".repeat(4 * 1024 * 1024 + 1)).pipe(
                match({onFailure: e => e.reason, onSuccess: () => "wrote"}))];
    "#
        ),
        json!(["too-large", "too-large"])
    );
    assert!(!workspace.root().join("oversize").exists());
}

#[test]
fn listings_are_bounded_sorted_and_hide_atomic_files() {
    let workspace = Workspace::new();
    fs::write(workspace.root().join("z"), "").unwrap();
    fs::create_dir(workspace.root().join("a")).unwrap();
    fs::write(workspace.root().join(".lucent-atomic-hidden"), "").unwrap();
    let host = workspace.host();
    assert_eq!(
        run(&host, "return yield* fs.list();"),
        json!([{"name":"a","kind":"directory"},{"name":"z","kind":"file"}])
    );
    for index in 0..998 {
        fs::write(workspace.root().join(format!("entry-{index:04}")), "").unwrap();
    }
    assert_eq!(run(&host, "return (yield* fs.list()).length;"), json!(1000));
    fs::write(workspace.root().join("overflow"), "").unwrap();
    assert_eq!(
        run(
            &host,
            r#"return yield* fs.list().pipe(match({onFailure: e => e.reason, onSuccess: () => "listed"}));"#
        ),
        json!("too-many-entries")
    );
}

#[test]
fn malformed_content_and_wrong_file_kinds_keep_their_reasons() {
    let workspace = Workspace::new();
    fs::write(workspace.root().join("utf8"), [0xff]).unwrap();
    fs::write(workspace.root().join("json"), "{").unwrap();
    fs::create_dir(workspace.root().join("directory")).unwrap();
    fs::write(workspace.root().join("directory/child"), "").unwrap();
    assert_eq!(
        run(
            &workspace.host(),
            r#"
        return yield* Effect.forEach([fs.readText("utf8"), fs.readJson("json"), fs.readText("directory"),
            fs.list("json"), fs.list("missing"), fs.remove("directory"), fs.writeJson("invalid", undefined)],
            effect => effect.pipe(match({onFailure: e => [e.reason, e.message], onSuccess: () => "ok"})));
    "#
        ),
        json!([
            [
                "invalid-utf8",
                "Filesystem read-text failed at utf8: invalid-utf8."
            ],
            [
                "invalid-json",
                "Filesystem read-json failed at json: invalid-json."
            ],
            [
                "not-file",
                "Filesystem read-text failed at directory: not-file."
            ],
            [
                "not-directory",
                "Filesystem list failed at json: not-directory."
            ],
            ["not-found", "Filesystem list failed at missing: not-found."],
            [
                "directory-not-empty",
                "Filesystem remove failed at directory: directory-not-empty."
            ],
            [
                "not-json-serializable",
                "Filesystem write-json failed at invalid: not-json-serializable."
            ]
        ])
    );
}

#[cfg(unix)]
#[test]
fn symlink_parents_and_targets_cannot_escape() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new();
    fs::create_dir(workspace.0.join("outside")).unwrap();
    fs::write(workspace.0.join("outside/secret"), "private").unwrap();
    symlink(workspace.0.join("outside"), workspace.root().join("link")).unwrap();
    symlink(
        workspace.0.join("outside/secret"),
        workspace.root().join("target"),
    )
    .unwrap();
    fs::write(workspace.root().join("inside"), "inside").unwrap();
    symlink("inside", workspace.root().join("relative")).unwrap();
    assert_eq!(
        run(
            &workspace.host(),
            r#"
        return yield* Effect.forEach([fs.writeText("link/escape", "bad"), fs.readText("link/secret"),
            fs.readText("target"), fs.writeText("target", "bad"), fs.list("link"), fs.readText("relative")],
            effect => effect.pipe(match({onFailure: e => e.reason, onSuccess: () => "escaped"})));
    "#
        ),
        json!(vec!["invalid-path"; 6])
    );
    assert_eq!(
        fs::read_to_string(workspace.0.join("outside/secret")).unwrap(),
        "private"
    );
    assert!(!workspace.0.join("outside/escape").exists());
}

fn start_body(host: &Host, body: &str) -> (lucent_host::Session, ScriptRun) {
    let (session, link) = testing::start_probe(host, ready_movie());
    log_in(&link);
    let source = format!(
        r#"
        import * as fs from "lucent/filesystem";
        import * as script from "lucent/script";
        import {{ Effect }} from "effect";
        const match = ({{onFailure, onSuccess}}) => effect => effect.pipe(Effect.map(onSuccess), Effect.catch(error => Effect.succeed(onFailure(error))));
        export default function* () {{ {body} }}
    "#
    );
    let run = ScriptRun::start(link, loose(&[("files.js", &source)]), Default::default()).unwrap();
    (session, run)
}

fn gate(
    host: &Host,
    path: &str,
    stage: testing::FileStage,
) -> (
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::SyncSender<()>,
) {
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let released = std::sync::Mutex::new(released);
    let path = path.to_string();
    host.set_file_probe(move |observed, name| {
        if observed == stage && name == path {
            entered.send(()).unwrap();
            released.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
        }
    });
    (waiting, release)
}

#[test]
fn concurrent_sessions_publish_whole_files_by_rename() {
    let workspace = Workspace::new();
    fs::write(workspace.root().join("shared"), "original").unwrap();
    let host = workspace.host();
    let (waiting, release) = gate(&host, "shared", testing::FileStage::BeforeRename);
    let (_a_session, a) = start_body(
        &host,
        r#"yield* fs.writeText("shared", "A".repeat(1024 * 1024));"#,
    );
    waiting.recv_timeout(TIMEOUT).unwrap();
    assert!(
        fs::read_to_string(workspace.root().join("shared")).unwrap() == "original",
        "destination changed before rename"
    );
    let (_b_session, b) = start_body(
        &host,
        r#"yield* fs.writeText("shared", "B".repeat(1024 * 1024));"#,
    );
    release.send(()).unwrap();
    waiting.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(
        fs::read_to_string(workspace.root().join("shared")).unwrap(),
        "A".repeat(1024 * 1024)
    );
    release.send(()).unwrap();
    assert_eq!(a.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    assert_eq!(b.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    assert_eq!(
        fs::read_to_string(workspace.root().join("shared")).unwrap(),
        "B".repeat(1024 * 1024)
    );
    assert_eq!(fs::read_dir(workspace.root()).unwrap().count(), 1);
}

#[test]
fn fiber_interruption_cancels_a_queued_request_before_issuance() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let released = std::sync::Mutex::new(released);
    let (cancelled, cancelled_rx) = std::sync::mpsc::sync_channel(1);
    host.set_file_probe(move |stage, path| {
        if stage == testing::FileStage::Issued && path == "blocker" {
            entered.send(()).unwrap();
            released.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
        }
        if stage == testing::FileStage::Cancelled && path == "cancelled" {
            cancelled.send(()).unwrap();
        }
    });
    let (_blocker_session, blocker) =
        start_body(&host, r#"yield* fs.writeText("blocker", "whole");"#);
    waiting.recv_timeout(TIMEOUT).unwrap();
    let (_session, interrupted) = start_body(
        &host,
        r#"
        yield* Effect.raceFirst(fs.writeText("cancelled", "bad"), Effect.tryPromise(() => Promise.resolve()));
        yield* fs.writeText("fence", "done");
    "#,
    );
    cancelled_rx.recv_timeout(TIMEOUT).unwrap();
    release.send(()).unwrap();
    assert_eq!(interrupted.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    assert_eq!(blocker.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    assert_eq!(
        run(&host, "return yield* fs.exists(\"cancelled\");"),
        json!(false)
    );
    assert_eq!(
        fs::read_to_string(workspace.root().join("fence")).unwrap(),
        "done"
    );
}

#[test]
fn run_teardown_discards_queued_work_and_finishes_an_issued_write() {
    stop_during_write(false);
    stop_during_write(true);
}

fn stop_during_write(hard: bool) {
    let workspace = Workspace::new();
    let host = workspace.host();
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let released = std::sync::Mutex::new(released);
    let (queued, queued_rx) = std::sync::mpsc::sync_channel(1);
    host.set_file_probe(move |stage, name| {
        if stage == testing::FileStage::BeforeRename && name == "issued" {
            entered.send(()).unwrap();
            released.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
        }
        if stage == testing::FileStage::Queued && name == "queued" {
            queued.send(()).unwrap();
        }
    });
    let (_session, mut stopped) = start_body(
        &host,
        r#"
        yield* Effect.raceFirst(fs.writeText("issued", "complete"), fs.writeText("queued", "bad"));
    "#,
    );
    waiting.recv_timeout(TIMEOUT).unwrap();
    queued_rx.recv_timeout(TIMEOUT).unwrap();
    if hard {
        stopped.kill();
        assert!(matches!(
            stopped.wait(TIMEOUT),
            Err(lucent_host::scripting::RuntimeError::Killed(_))
        ));
    } else {
        stopped.stop_script();
        assert_eq!(
            stopped.wait(TIMEOUT).unwrap(),
            ScriptOutcome::Stopped { reason: None }
        );
    }
    stopped.join_cpu_time();
    release.send(()).unwrap();
    assert_eq!(
        run(
            &host,
            "return [yield* fs.readText(\"issued\"), yield* fs.exists(\"queued\")];"
        ),
        json!(["complete", false])
    );
}

#[test]
fn files_at_the_byte_limit_and_json_null_keep_their_values() {
    let workspace = Workspace::new();
    fs::write(workspace.root().join("bom"), "\u{feff}héllo").unwrap();
    fs::write(workspace.root().join("null.json"), "null").unwrap();
    assert_eq!(
        run(
            &workspace.host(),
            r#"
        yield* fs.writeText("limit", "x".repeat(8 * 1024 * 1024));
        const contents = yield* fs.readText("limit");
        return [contents.length, contents === "x".repeat(8 * 1024 * 1024),
            yield* fs.readJson("null.json"), yield* fs.readText("bom")];
    "#
        ),
        json!([8 * 1024 * 1024, true, null, "héllo"])
    );
}

#[cfg(unix)]
#[test]
fn created_directories_and_files_have_private_modes() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    fs::remove_dir(workspace.root()).unwrap();
    let host = workspace.host();
    assert_eq!(host.data_root(), workspace.root());
    assert_eq!(
        run(
            &host,
            r#"yield* fs.writeText("nested/private", "saved"); return yield* fs.readText("nested/private");"#
        ),
        json!("saved")
    );
    for (path, mode) in [
        (workspace.root(), 0o700),
        (workspace.root().join("nested"), 0o700),
        (workspace.root().join("nested/private"), 0o600),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
}

#[cfg(unix)]
#[test]
fn symlink_entries_can_be_listed_and_removed_without_touching_targets() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new();
    fs::write(workspace.0.join("secret"), "private").unwrap();
    symlink(workspace.0.join("secret"), workspace.root().join("link")).unwrap();
    assert_eq!(
        run(
            &workspace.host(),
            r#"
        const result = [yield* fs.exists("link"), yield* fs.list()];
        yield* fs.remove("link");
        return [...result, yield* fs.exists("link")];
    "#
        ),
        json!([true,[{"name":"link","kind":"symbolic-link"}],false])
    );
    assert_eq!(
        fs::read_to_string(workspace.0.join("secret")).unwrap(),
        "private"
    );
}

#[cfg(unix)]
#[test]
fn an_invalid_data_root_is_never_followed() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new();
    fs::remove_dir(workspace.root()).unwrap();
    fs::write(workspace.root(), "not a directory").unwrap();
    let host = workspace.host();
    let read_reason = r#"return yield* fs.readText("value").pipe(match({onFailure: e => e.reason, onSuccess: () => "read"}));"#;
    assert_eq!(run(&host, read_reason), json!("not-directory"));
    fs::remove_file(workspace.root()).unwrap();
    symlink(workspace.0.join("outside"), workspace.root()).unwrap();
    assert_eq!(run(&host, read_reason), json!("invalid-path"));
    assert!(!workspace.0.join("outside").exists());
}

#[test]
fn text_uses_utf8_replacement_for_unpaired_surrogates() {
    let workspace = Workspace::new();
    assert_eq!(
        run(
            &workspace.host(),
            r#"
        yield* fs.writeText("replacement", "\ud800x\udfff");
        return (yield* fs.readText("replacement")) === "\ufffdx\ufffd";
    "#
        ),
        json!(true)
    );
}

#[test]
fn json_reads_preserve_javascript_strings_and_nested_values() {
    let workspace = Workspace::new();
    fs::write(workspace.root().join("surrogate.json"), r#""\ud800""#).unwrap();
    fs::write(
        workspace.root().join("deep.json"),
        format!("{}0{}", "[".repeat(150), "]".repeat(150)),
    )
    .unwrap();
    assert_eq!(
        run(
            &workspace.host(),
            r#"
        const text = yield* fs.readJson("surrogate.json");
        let nested = yield* fs.readJson("deep.json");
        for (let index = 0; index < 150; index++) nested = nested[0];
        yield* fs.writeJson("round-trip.json", {name: "\ud800"});
        return [text.charCodeAt(0), nested, (yield* fs.readJson("round-trip.json")).name.charCodeAt(0)];
    "#
        ),
        json!([55296, 0, 55296])
    );
}

#[test]
fn full_host_file_queue_rejects_without_blocking_scripts_or_sessions() {
    let logs = common::logs::Logs::default();
    let dispatch = logs.dispatch();
    let _guard = tracing::dispatcher::set_default(&dispatch);
    let workspace = Workspace::new();
    let host = workspace.host();
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let released = std::sync::Mutex::new(released);
    let (queued, queued_rx) = std::sync::mpsc::channel();
    host.set_file_probe(move |stage, path| {
        if stage == testing::FileStage::BeforeRename && path == "blocker" {
            entered.send(()).unwrap();
            let _ = released.lock().unwrap().recv();
        }
        if stage == testing::FileStage::Queued && path.starts_with("pending") {
            queued.send(path.to_owned()).unwrap();
        }
    });
    let (_blocker_session, blocker) =
        start_body(&host, r#"yield* fs.writeText("blocker", "whole");"#);
    waiting.recv_timeout(TIMEOUT).unwrap();
    let mut runs = Vec::new();
    for session in 0..4 {
        runs.push(start_body(
            &host,
            &format!(
                r#"
            let completed = 0, finish;
            const done = new Promise(resolve => {{ finish = resolve; }});
            const requests = Array.from({{length: 16}}, (_, i) =>
                fs.writeText("pending-{session}-" + i, "whole").pipe(
                    Effect.tap(() => Effect.sync(() => {{ if (++completed === 16) finish(); }})),
                    Effect.flatMap(() => Effect.tryPromise(() => done))));
            yield* requests.reduceRight((rest, request) => Effect.raceFirst(request, rest),
                Effect.tryPromise(() => done));
        "#
            ),
        ));
        for _ in 0..16 {
            queued_rx.recv_timeout(TIMEOUT).unwrap();
        }
    }
    assert_eq!(
        run(
            &host,
            r#"
        return [yield* fs.exists("overflow").pipe(match({onFailure: e => e.reason, onSuccess: () => "admitted"})), 1 + 1];
    "#
        ),
        json!(["busy", 2])
    );
    let (mut session, observer) = testing::start_probe(&host, ready_movie());
    log_in(&observer);
    let (request, controls) = lucent_host::ScriptRequest::new(loose(&[(
        "responsive.js",
        r#"
        import * as api from "lucent/api";
        export default function* () {
            yield* api.packet.sendToClient("started");
        }
    "#,
    )]));
    let _supervisor = lucent_host::Supervisor::start(&mut session, Some(request)).unwrap();
    assert_eq!(
        await_event(&observer, "onDebug", None).args,
        vec![lucent_protocol::bridge::Value::String("started".into())]
    );
    logs.wait_for(
        tracing::Level::DEBUG,
        &format!("session {}: script worker reaped", observer.id()),
    );
    controls.status();
    logs.wait_for(
        tracing::Level::INFO,
        &format!("session {}: no script is running", observer.id()),
    );
    release.send(()).unwrap();
    assert_eq!(blocker.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    for (_session, run) in runs {
        assert_eq!(run.wait(TIMEOUT).unwrap(), ScriptOutcome::Completed);
    }
    assert_eq!(fs::read_dir(workspace.root()).unwrap().count(), 65);
    assert_eq!(
        run(&host, r#"return yield* fs.exists("blocker");"#),
        json!(true)
    );
}
