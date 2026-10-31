mod common;

use lucent_host::library::{Package, load_script};
use lucent_protocol::script::{ModulePath, Script, ScriptOutcome};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lucent-library-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn package(&self, dir: &str, name: &str) {
        self.write(
            &format!("{dir}/package.json"),
            &format!(r#"{{"name":"{name}","main":"./index.js","lucent":{{}}}}"#),
        );
        self.write(&format!("{dir}/index.js"), "export const value = 42;");
        self.write(
            &format!("{dir}/scripts/run.js"),
            "export default function* () {}",
        );
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn a_daily_loads_by_file_and_by_manifest_name() {
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for spec in [
        "script-packages/lucent-dailies/scripts/farm-elders-blood.js",
        "@lucent/dailies/scripts/farm-elders-blood.js",
    ] {
        let script = load_script(spec, &cwd).unwrap();
        assert_eq!(
            script.entry().to_string(),
            "@lucent/dailies/scripts/farm-elders-blood.js"
        );
        assert_eq!(
            script
                .resolve(script.entry(), "@lucent/dailies")
                .unwrap()
                .to_string(),
            "@lucent/dailies/index.js"
        );
        assert_eq!(
            script
                .resolve(script.entry(), "@lucent/consumables")
                .unwrap()
                .to_string(),
            "@lucent/consumables/index.js"
        );
    }
}

#[test]
fn loose_files_include_the_shared_packages_without_reading_siblings() {
    let ws = Workspace::new();
    ws.write("package.json", r#"{"name":"workspace"}"#);
    ws.write("loop.js", "export default function* () {}");
    ws.write("sibling.js", "export const sibling = true;");
    let script = load_script("loop.js", &ws.0).unwrap();
    assert_eq!(script.entry().to_string(), "loop.js");
    assert!(script.resolve(script.entry(), "@lucent/dailies").is_none());
    assert!(script.resolve(script.entry(), "./sibling.js").is_none());
    ws.package("script-packages/unrelated-folder", "@lucent/dailies");
    let script = load_script("loop.js", &ws.0).unwrap();
    assert_eq!(
        script
            .resolve(script.entry(), "@lucent/dailies")
            .unwrap()
            .to_string(),
        "@lucent/dailies/index.js"
    );
    let script = load_script("@lucent/dailies/scripts/run.js", &ws.0).unwrap();
    assert_eq!(script.entry().to_string(), "@lucent/dailies/scripts/run.js");
    ws.package("script-packages/unscoped", "tools");
    assert_eq!(
        load_script("tools/scripts/run.js", &ws.0)
            .unwrap()
            .entry()
            .to_string(),
        "tools/scripts/run.js"
    );
}

#[test]
fn packages_load_modules_and_skip_dependencies_and_hidden_entries() {
    let ws = Workspace::new();
    ws.package("pkg", "@me/pkg");
    ws.write("pkg/lib/a.mjs", "export const a = 1;");
    ws.write("pkg/node_modules/x/index.js", "excluded");
    ws.write("pkg/.hidden/index.js", "excluded");
    let package = Package::read(&ws.0.join("pkg")).unwrap();
    assert_eq!(package.main.to_string(), "@me/pkg/index.js");
    let script = package.script("scripts/run.js").unwrap();
    assert_eq!(
        script.source(&ModulePath::in_package("@me/pkg", "lib/a.mjs").unwrap()),
        Some("export const a = 1;")
    );
    for path in ["node_modules/x/index.js", ".hidden/index.js"] {
        assert!(
            script
                .source(&ModulePath::in_package("@me/pkg", path).unwrap())
                .is_none()
        );
    }
    assert_eq!(
        load_script("pkg/scripts/run.js", &ws.0)
            .unwrap()
            .entry()
            .to_string(),
        "@me/pkg/scripts/run.js"
    );
}

#[test]
fn duplicate_names_and_missing_specs_name_the_input_and_search_root() {
    let ws = Workspace::new();
    ws.package("script-packages/a", "@me/pkg");
    ws.package("script-packages/b", "@me/pkg");
    let error = load_script("@me/pkg/scripts/run.js", &ws.0)
        .unwrap_err()
        .to_string();
    assert!(error.contains("duplicate package name @me/pkg"), "{error}");
    fs::remove_dir_all(ws.0.join("script-packages/b")).unwrap();
    for spec in [
        "@missing/pkg/run.js",
        "missing.js",
        "@me/pkg/scripts/missing.js",
    ] {
        let error = load_script(spec, &ws.0).unwrap_err().to_string();
        assert!(error.contains(spec), "{error}");
        assert!(
            error.contains(ws.0.join("script-packages").to_str().unwrap()),
            "{error}"
        );
    }
}

#[test]
fn malformed_manifests_report_the_manifest_and_reason() {
    let ws = Workspace::new();
    ws.write("package.json", r#"{"name":"@me/pkg","lucent":{}}"#);
    let error = Package::read(&ws.0).unwrap_err().to_string();
    assert!(
        error.ends_with("is not a Lucent package manifest: \"main\" is missing"),
        "{error}"
    );
}

fn sized_file(ws: &Workspace, path: &str, bytes: u64) {
    ws.write(path, "");
    fs::OpenOptions::new()
        .write(true)
        .open(ws.0.join(path))
        .unwrap()
        .set_len(bytes)
        .unwrap();
}

fn load_error(ws: &Workspace, spec: &str, reason: &str) {
    assert_eq!(
        load_script(spec, &ws.0)
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        format!(
            "couldn't load script {spec:?}; searched {}: {reason}",
            ws.0.join("script-packages").display()
        )
    );
}

#[test]
fn script_file_size_accepts_the_boundary_and_rejects_one_more_byte() {
    let ws = Workspace::new();
    sized_file(&ws, "run.js", 16 << 20);
    assert_eq!(
        load_script("run.js", &ws.0)
            .unwrap()
            .source(&ModulePath::loose("run.js").unwrap())
            .unwrap()
            .len(),
        16 << 20
    );
    sized_file(&ws, "run.js", (16 << 20) + 1);
    load_error(
        &ws,
        "run.js",
        &format!(
            "Script file exceeds the 16 MiB limit: {}.",
            ws.0.join("run.js").canonicalize().unwrap().display()
        ),
    );
}

#[test]
fn manifest_size_accepts_the_boundary_and_rejects_one_more_byte() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    let path = ws.0.join("pkg/package.json");
    let mut source = fs::read_to_string(&path).unwrap();
    source.extend(std::iter::repeat_n(' ', (1 << 20) - source.len()));
    fs::write(&path, &source).unwrap();
    assert_eq!(Package::read(&ws.0.join("pkg")).unwrap().name, "tools");
    source.push(' ');
    fs::write(&path, source).unwrap();
    assert_eq!(
        Package::read(&ws.0.join("pkg"))
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        "package.json exceeds the 1 MiB limit."
    );
}

#[test]
fn package_file_size_includes_non_source_assets() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    sized_file(&ws, "pkg/asset.bin", 64 << 20);
    assert_eq!(Package::read(&ws.0.join("pkg")).unwrap().modules.len(), 2);
    sized_file(&ws, "pkg/asset.bin", (64 << 20) + 1);
    assert_eq!(
        Package::read(&ws.0.join("pkg"))
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        format!(
            "File exceeds the 64 MiB limit: {}.",
            ws.0.join("pkg").join("asset.bin").display()
        )
    );
}

#[test]
fn package_total_size_counts_the_manifest_and_all_assets() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    let overhead = ["package.json", "index.js", "scripts/run.js"]
        .iter()
        .map(|p| fs::metadata(ws.0.join("pkg").join(p)).unwrap().len())
        .sum::<u64>();
    for i in 0..4 {
        sized_file(
            &ws,
            &format!("pkg/{i}.bin"),
            (64 << 20) - if i == 0 { overhead } else { 0 },
        );
    }
    assert_eq!(Package::read(&ws.0.join("pkg")).unwrap().modules.len(), 2);
    sized_file(&ws, "pkg/0.bin", (64 << 20) - overhead + 1);
    assert_eq!(
        Package::read(&ws.0.join("pkg"))
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        "The package exceeds the 256 MiB limit."
    );
}

#[test]
fn package_file_count_stops_before_reading_any_source() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    for i in 0..9_997 {
        ws.write(&format!("pkg/{i}.bin"), "");
    }
    assert_eq!(Package::read(&ws.0.join("pkg")).unwrap().modules.len(), 2);
    ws.write("pkg/extra.bin", "");
    fs::write(ws.0.join("pkg/index.js"), [255]).unwrap();
    fs::write(ws.0.join("pkg/scripts/run.js"), [255]).unwrap();
    assert_eq!(
        Package::read(&ws.0.join("pkg"))
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        format!(
            "Directory contains more than 10000 files: {}.",
            ws.0.join("pkg").display()
        )
    );
}

#[test]
fn oversized_package_totals_stop_before_reading_any_source() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    fs::write(ws.0.join("pkg/index.js"), [255]).unwrap();
    fs::write(ws.0.join("pkg/scripts/run.js"), [255]).unwrap();
    for i in 0..5 {
        sized_file(&ws, &format!("pkg/{i}.bin"), 64 << 20);
    }
    assert_eq!(
        Package::read(&ws.0.join("pkg"))
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        "The package exceeds the 256 MiB limit."
    );
}

#[test]
fn module_path_size_is_measured_in_utf8_bytes() {
    let ws = Workspace::new();
    let prefix = format!(
        "{}/{}/{}/d/",
        "a".repeat(255),
        "b".repeat(255),
        "c".repeat(255)
    );
    let boundary = format!("{prefix}{}.jsx", "é".repeat(125));
    assert_eq!(boundary.len(), 1024);
    ws.write(
        "pkg/package.json",
        &format!(r#"{{"name":"tools","main":"{boundary}","lucent":{{}}}}"#),
    );
    assert_eq!(
        Package::read(&ws.0.join("pkg")).unwrap().main.path(),
        boundary
    );
    let over = format!("{boundary}x");
    ws.write(
        "pkg/package.json",
        &format!(r#"{{"name":"tools","main":"{over}","lucent":{{}}}}"#),
    );
    assert_eq!(
        Package::read(&ws.0.join("pkg"))
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        "Module path exceeds the 1 KiB limit."
    );
}

#[test]
fn manifest_module_components_accept_255_bytes_and_reject_256() {
    let ws = Workspace::new();
    for (bytes, expected) in [
        (255, None),
        (
            256,
            Some("Module path component exceeds the 255 bytes limit."),
        ),
    ] {
        let main = format!("{}.js", "é".repeat(126)) + &"x".repeat(bytes - 255);
        ws.write(
            "pkg/package.json",
            &format!(r#"{{"name":"tools","main":"{main}","lucent":{{}}}}"#),
        );
        let result = Package::read(&ws.0.join("pkg"));
        if let Some(expected) = expected {
            assert_eq!(result.map(|_| ()).unwrap_err().to_string(), expected);
        } else {
            assert_eq!(result.unwrap().main.path().len(), 255);
        }
    }
}

fn padded_source(ws: &Workspace, path: &str, prefix: &str, bytes: usize) {
    let mut source = prefix.to_owned();
    source.extend(std::iter::repeat_n(' ', bytes - source.len()));
    ws.write(path, &source);
}

fn outcome(script: Script) -> ScriptOutcome {
    let (_host, mut session, _runtime) = common::ready_runtime();
    common::start(&mut session, script, Default::default())
        .wait(common::TIMEOUT)
        .unwrap()
}

fn source_limit(script: Script) {
    match outcome(script) {
        ScriptOutcome::Failed { message, .. } => {
            assert_eq!(message, "Script sources exceed the 16 MiB execution limit.");
        }
        other => panic!("expected a source limit failure, got {other:?}"),
    }
}

#[test]
fn snapshot_counts_only_reachable_sources_once_across_packages() {
    let ws = Workspace::new();
    for name in ["a", "b", "unused"] {
        ws.package(&format!("script-packages/{name}"), name);
    }
    padded_source(
        &ws,
        "script-packages/a/index.js",
        "export * from 'b'; import 'b'; export default function* () {}",
        8 << 20,
    );
    padded_source(
        &ws,
        "script-packages/b/index.js",
        "import 'a'; export const b = 1;",
        8 << 20,
    );
    padded_source(
        &ws,
        "script-packages/unused/index.js",
        "export {};",
        16 << 20,
    );
    ws.write("run.js", "export default function* () {}");
    assert_eq!(
        outcome(load_script("run.js", &ws.0).unwrap()),
        ScriptOutcome::Completed
    );
    assert_eq!(
        outcome(load_script("a/index.js", &ws.0).unwrap()),
        ScriptOutcome::Completed
    );
    ws.write(
        "run.js",
        "import 'a'; import 'b'; export default function* () {}",
    );
    source_limit(load_script("run.js", &ws.0).unwrap());
}

#[test]
fn package_javascript_obeys_the_script_file_limit() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    sized_file(&ws, "pkg/index.js", 16 << 20);
    assert_eq!(Package::read(&ws.0.join("pkg")).unwrap().modules.len(), 2);
    sized_file(&ws, "pkg/index.js", (16 << 20) + 1);
    assert_eq!(
        Package::read(&ws.0.join("pkg"))
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        format!(
            "Script file exceeds the 16 MiB limit: {}.",
            ws.0.join("pkg").join("index.js").display()
        )
    );
}

#[test]
fn package_scripts_apply_the_reachable_snapshot_limit() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    padded_source(
        &ws,
        "pkg/index.js",
        "export * from './scripts/run';",
        8 << 20,
    );
    padded_source(
        &ws,
        "pkg/scripts/run.js",
        "import '../index.js'; export default function* () {}",
        8 << 20,
    );
    assert_eq!(
        outcome(
            Package::read(&ws.0.join("pkg"))
                .unwrap()
                .script("scripts/run.js")
                .unwrap()
        ),
        ScriptOutcome::Completed
    );
    padded_source(
        &ws,
        "pkg/index.js",
        "export * from './scripts/run';",
        (8 << 20) + 1,
    );
    source_limit(
        Package::read(&ws.0.join("pkg"))
            .unwrap()
            .script("scripts/run.js")
            .unwrap(),
    );
}

#[cfg(unix)]
#[test]
fn symlinked_package_manifests_are_rejected() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    fs::rename(ws.0.join("pkg/package.json"), ws.0.join("manifest.json")).unwrap();
    std::os::unix::fs::symlink(ws.0.join("manifest.json"), ws.0.join("pkg/package.json")).unwrap();
    assert_eq!(
        Package::read(&ws.0.join("pkg"))
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        format!(
            "{} is not a Lucent package manifest: package.json must be a file.",
            ws.0.join("pkg").join("package.json").display()
        )
    );
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn walked_and_loose_components_are_limited_in_utf8_bytes() {
    let ws = Workspace::new();
    for extra in ["", "x"] {
        let name = format!("{}{}.js", "é".repeat(126), extra);
        ws.write(&name, "export default function* () {}");
        if extra.is_empty() {
            assert_eq!(load_script(&name, &ws.0).unwrap().entry().path(), name);
        } else {
            load_error(
                &ws,
                &name,
                "Module path component exceeds the 255 bytes limit.",
            );
        }
        fs::remove_file(ws.0.join(&name)).unwrap();
        ws.package("pkg", "tools");
        ws.write(&format!("pkg/{name}"), "export {};");
        if extra.is_empty() {
            assert_eq!(Package::read(&ws.0.join("pkg")).unwrap().modules.len(), 3);
        } else {
            assert_eq!(
                Package::read(&ws.0.join("pkg"))
                    .map(|_| ())
                    .unwrap_err()
                    .to_string(),
                "Module path component exceeds the 255 bytes limit."
            );
        }
        fs::remove_file(ws.0.join("pkg").join(name)).unwrap();
    }
}

#[cfg(target_os = "linux")]
#[test]
fn walked_paths_enforce_one_kib_on_linux_with_long_os_paths() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    let prefix = format!("{}/", "a".repeat(200)).repeat(5);
    let boundary = format!("{prefix}{}.js", "b".repeat(16));
    assert_eq!(boundary.len(), 1024);
    ws.write(&format!("pkg/{boundary}"), "export {};");
    assert_eq!(Package::read(&ws.0.join("pkg")).unwrap().modules.len(), 3);
    fs::remove_file(ws.0.join("pkg").join(&boundary)).unwrap();
    ws.write(&format!("pkg/{boundary}x"), "");
    assert_eq!(
        Package::read(&ws.0.join("pkg"))
            .map(|_| ())
            .unwrap_err()
            .to_string(),
        "Module path exceeds the 1 KiB limit."
    );
    let loose = format!("{prefix}run.js");
    ws.write(&loose, "export default function* () {}");
    assert_eq!(load_script(&loose, &ws.0).unwrap().entry().path(), "run.js");
}

#[test]
fn snapshot_follows_escaped_imports_but_ignores_import_text_and_dynamic_imports() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    padded_source(&ws, "pkg/index.js", "export const value = 42;", 8 << 20);
    let entry = r#"import '../index\u002ejs';
        const text = "import './extra.js'";
        const template = `export * from './extra.js'`;
        const regex = /import 'extra'/;
        if (false) import('./extra.js');
        export default function* () {}"#;
    padded_source(&ws, "pkg/scripts/run.js", entry, 8 << 20);
    padded_source(&ws, "pkg/scripts/extra.js", "export {};", 16 << 20);
    assert_eq!(
        outcome(
            Package::read(&ws.0.join("pkg"))
                .unwrap()
                .script("scripts/run.js")
                .unwrap()
        ),
        ScriptOutcome::Completed
    );
    padded_source(
        &ws,
        "pkg/index.js",
        "export const value = 42;",
        (8 << 20) + 1,
    );
    source_limit(
        Package::read(&ws.0.join("pkg"))
            .unwrap()
            .script("scripts/run.js")
            .unwrap(),
    );
}

#[test]
fn loaded_reachable_graph_runs_with_the_runtime_module_resolver() {
    let ws = Workspace::new();
    ws.package("pkg", "tools");
    ws.write("pkg/scripts/run.js", "import { value } from '../index'; export default function* () { if (value !== 42) throw new Error('wrong module'); }");
    let script = load_script("pkg/scripts/run.js", &ws.0).unwrap();
    let (_host, mut session, _runtime) = common::ready_runtime();
    assert_eq!(
        common::start(&mut session, script, Default::default())
            .wait(std::time::Duration::from_secs(5))
            .unwrap(),
        lucent_protocol::script::ScriptOutcome::Completed
    );
}

fn mixed_source_limit(files: usize) {
    let mut modules = std::collections::BTreeMap::new();
    for i in 0..files {
        let mut source = if i == 0 {
            let mut entry = format!(
                "export default function* () {{ if (globalThis.count !== {files}) throw new Error('missing module'); }}\n"
            );
            for j in 1..files {
                entry.push_str(&format!("import './{j}.js';\n"));
            }
            entry
        } else {
            String::new()
        };
        source.push_str("globalThis.count = (globalThis.count ?? 0) + 1;\n/*Ā");
        let bytes = (24 << 20) / files;
        source.extend(std::iter::repeat_n(' ', bytes - source.len() - 2));
        source.push_str("*/");
        modules.insert(ModulePath::loose(format!("{i}.js")).unwrap(), source);
    }
    source_limit(
        Script::new(
            ModulePath::loose("0.js").unwrap(),
            modules,
            Default::default(),
        )
        .unwrap(),
    );
}

#[test]
fn four_mixed_sources_fail_the_run_without_a_heap_kill() {
    mixed_source_limit(4);
}

#[test]
fn eight_mixed_sources_fail_the_run_without_a_heap_kill() {
    mixed_source_limit(8);
}
