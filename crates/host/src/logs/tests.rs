use super::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

fn empty_dir() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "lucent-logs-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn files(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                std::fs::read_to_string(entry.path()).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

fn record(text: &str, bytes: usize) -> Vec<u8> {
    let mut line = text.repeat(bytes - 1).into_bytes();
    line.truncate(bytes - 1);
    line.push(b'\n');
    line
}

fn owned(files: &[(&str, &str)]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(name, text)| (name.to_string(), text.to_string()))
        .collect()
}

#[test]
fn records_share_a_file_until_the_next_would_pass_the_limit() {
    let dir = empty_dir();
    let mut log = RotatingFile::open(dir.clone(), 100).unwrap();
    log.write_all(&record("a", 60)).unwrap();
    log.write_all(&record("b", 40)).unwrap();
    assert_eq!(files(&dir).len(), 1, "exactly the limit stays in one file");
    log.write_all(&record("c", 1)).unwrap();
    let a = String::from_utf8(record("a", 60)).unwrap();
    let b = String::from_utf8(record("b", 40)).unwrap();
    assert_eq!(
        files(&dir),
        owned(&[("lucent.log", "\n"), ("lucent.log.1", &(a + &b))])
    );
}

#[test]
fn rotation_keeps_the_three_newest_backups() {
    let dir = empty_dir();
    let mut log = RotatingFile::open(dir.clone(), 10).unwrap();
    for text in ["0", "1", "2", "3", "4", "5"] {
        log.write_all(&record(text, 10)).unwrap();
    }
    let line = |text| String::from_utf8(record(text, 10)).unwrap();
    assert_eq!(
        files(&dir),
        vec![
            ("lucent.log".into(), line("5")),
            ("lucent.log.1".into(), line("4")),
            ("lucent.log.2".into(), line("3")),
            ("lucent.log.3".into(), line("2")),
        ]
    );
}

#[test]
fn a_record_larger_than_the_limit_lands_whole_in_a_fresh_file() {
    let dir = empty_dir();
    let mut log = RotatingFile::open(dir.clone(), 10).unwrap();
    log.write_all(&record("a", 5)).unwrap();
    log.write_all(&record("b", 50)).unwrap();
    assert_eq!(
        files(&dir),
        vec![
            (
                "lucent.log".into(),
                String::from_utf8(record("b", 50)).unwrap()
            ),
            (
                "lucent.log.1".into(),
                String::from_utf8(record("a", 5)).unwrap()
            ),
        ]
    );
}

#[test]
fn a_restart_appends_and_counts_what_the_file_already_holds() {
    let dir = empty_dir();
    RotatingFile::open(dir.clone(), 100)
        .unwrap()
        .write_all(&record("a", 60))
        .unwrap();
    let mut log = RotatingFile::open(dir.clone(), 100).unwrap();
    log.write_all(&record("b", 30)).unwrap();
    assert_eq!(files(&dir).len(), 1);
    log.write_all(&record("c", 30)).unwrap();
    let a = String::from_utf8(record("a", 60)).unwrap();
    let b = String::from_utf8(record("b", 30)).unwrap();
    assert_eq!(
        files(&dir)[1],
        ("lucent.log.1".to_string(), a + &b),
        "the restart appended to the old file before rotating it"
    );
}

#[cfg(unix)]
#[test]
fn log_files_are_readable_only_by_their_owner() {
    use std::os::unix::fs::PermissionsExt;
    let dir = empty_dir();
    let mut log = RotatingFile::open(dir.clone(), 10).unwrap();
    log.write_all(&record("a", 10)).unwrap();
    log.write_all(&record("b", 10)).unwrap();
    for name in ["lucent.log", "lucent.log.1"] {
        let mode = std::fs::metadata(dir.join(name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{name}");
    }
}

fn logged(debug: bool, env: Option<&str>, emit: impl FnOnce()) -> Vec<serde_json::Value> {
    let dir = empty_dir();
    let log = Mutex::new(RotatingFile::open(dir.clone(), MAX_FILE_BYTES).unwrap());
    let subscriber = tracing_subscriber::registry()
        .with(filter(debug, env.map(str::to_owned)))
        .with(json_layer(log));
    tracing::subscriber::with_default(subscriber, emit);
    std::fs::read_to_string(dir.join(FILE_NAME))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn each_record_is_one_json_line_with_its_level_target_and_fields() {
    let records = logged(false, None, || {
        tracing::info!(target: "lucent_host::sessions", session = 3, "supervisor started");
        tracing::warn!(target: "lucent::account_manager", error = "denied", "couldn't read accounts");
    });
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["level"], "INFO");
    assert_eq!(records[0]["target"], "lucent_host::sessions");
    assert_eq!(records[0]["message"], "supervisor started");
    assert_eq!(records[0]["session"], 3);
    assert!(records[0]["timestamp"].is_string());
    assert_eq!(records[1]["level"], "WARN");
    assert_eq!(records[1]["error"], "denied");
}

fn debug_targets(debug: bool, env: Option<&str>) -> Vec<String> {
    logged(debug, env, || {
        tracing::debug!(target: "lucent_host::bridge", "host debug");
        tracing::debug!(target: "lucent::game_window", "app debug");
        tracing::debug!(target: "lucent_ui::text_field", "ui debug");
        tracing::debug!(target: "lucent_host::scripting::script", "script debug");
        tracing::debug!(target: "wgpu_core::device", "dependency debug");
        tracing::info!(target: "wgpu_core::device", "dependency info");
    })
    .into_iter()
    .map(|record| record["message"].as_str().unwrap().to_owned())
    .collect()
}

#[test]
fn debug_records_from_lucent_need_the_debug_flag() {
    assert_eq!(
        debug_targets(false, None),
        ["script debug", "dependency info"]
    );
    assert_eq!(
        debug_targets(true, None),
        [
            "host debug",
            "app debug",
            "ui debug",
            "script debug",
            "dependency info"
        ]
    );
}

#[test]
fn rust_log_overrides_both_defaults() {
    assert_eq!(debug_targets(true, Some("warn")), Vec::<String>::new());
    assert_eq!(
        debug_targets(false, Some("wgpu_core=debug")),
        ["dependency debug", "dependency info"]
    );
}

#[test]
fn a_script_line_keeps_its_first_16_kib_and_says_how_much_it_cut() {
    assert_eq!(script_line("short".into()), "short");
    let exact = "a".repeat(16 * 1024);
    assert_eq!(script_line(exact.clone()), exact);
    let long = format!("{}{}", "a".repeat(16 * 1024 - 1), "é".repeat(1000));
    assert_eq!(
        script_line(long),
        format!("{}… (2000 more bytes)", "a".repeat(16 * 1024 - 1))
    );
}
