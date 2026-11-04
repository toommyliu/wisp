mod common;

use common::app_data::{Workspace, wait};
use lucent_host::accounts::{AccountEdit, Password};
use lucent_host::bridge::Value;
use lucent_host::testing::{self, ProbeMovie};
use lucent_host::{Host, ScriptLink};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(15);
const HERO: &str =
    "lucent-game-account-ae6c79d10f1fd410650790e63186ec108fa106325b52fbd88de21a43540e6f2c";

fn text(value: &str) -> Value {
    Value::String(value.into())
}

fn save(link: &ScriptLink, name: &str, key: &str, value: &str) {
    let saved = link
        .call_loader("probe.saveShared", vec![text(name), text(key), text(value)])
        .recv_timeout(TIMEOUT);
    assert_eq!(saved, Ok(Value::Bool(true)));
}

fn read(link: &ScriptLink, name: &str, key: &str) -> Value {
    link.call_loader("probe.readShared", vec![text(name), text(key)])
        .recv_timeout(TIMEOUT)
        .unwrap()
}

fn read_as(host: &Host, account: Option<&str>, name: &str, key: &str) -> Value {
    let (_session, link) = match account {
        Some(username) => testing::start_account_probe(host, ProbeMovie::new(), username),
        None => testing::start_probe(host, ProbeMovie::new()),
    };
    read(&link, name, key)
}

fn saved_as(workspace: &Workspace, account: Option<&str>, name: &str, key: &str, value: &str) {
    let host = workspace.host();
    let (session, link) = match account {
        Some(username) => testing::start_account_probe(&host, ProbeMovie::new(), username),
        None => testing::start_probe(&host, ProbeMovie::new()),
    };
    save(&link, name, key, value);
    drop((link, session, host));
}

fn local_storage(workspace: &Workspace, partition: &str) -> PathBuf {
    ["Partitions", partition, "Local Storage", "leveldb"]
        .iter()
        .fold(workspace.release(), |path, part| path.join(part))
}

fn latin1(text: &str) -> Vec<u8> {
    std::iter::once(1).chain(text.bytes()).collect()
}

fn utf16(text: &str) -> Vec<u8> {
    std::iter::once(0)
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect()
}

fn ruffle_web_saves(workspace: &Workspace, partition: &str, saves: &[(&str, &[u8])], wide: bool) {
    use base64::Engine;
    let path = local_storage(workspace, partition);
    fs::create_dir_all(&path).unwrap();
    let options = rusty_leveldb::Options {
        create_if_missing: true,
        ..rusty_leveldb::Options::default()
    };
    let mut db = rusty_leveldb::DB::open(&path, options).unwrap();
    let encode = if wide { utf16 } else { latin1 };
    for (name, bytes) in saves {
        let mut key = b"_file://\0".to_vec();
        key.extend(encode(&format!("assets//{name}")));
        let value = encode(&base64::engine::general_purpose::STANDARD.encode(bytes));
        db.put(&key, &value).unwrap();
    }
    let chromium_rows: [(&[u8], &[u8]); 5] = [
        (b"VERSION", b"1"),
        (b"META:file://", b"\x08\x01\x10\x02"),
        (b"METAACCESS:file://", b"\x08\x01"),
        (b"_devtools://devtools\0\x01consoleHistory", b"\x01[]"),
        (b"_devtools://devtools\0\x01a//b", b"\x01QUJD"),
    ];
    for (key, value) in chromium_rows {
        db.put(key, value).unwrap();
    }
    db.flush().unwrap();
}

fn produced(name: &str, key: &str, value: &str) -> Vec<u8> {
    let source = Workspace::new();
    saved_as(&source, Some("Hero"), name, key, value);
    fs::read(source.path(&format!("saved-objects/accounts/hero/localhost/{name}.sol"))).unwrap()
}

fn snapshot(path: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

#[test]
fn a_game_save_outlives_the_host() {
    let workspace = Workspace::new();
    saved_as(&workspace, None, "AQWUserPref", "quality", "LOW");

    assert!(
        workspace
            .path("saved-objects/default/localhost/AQWUserPref.sol")
            .is_file()
    );
    assert_eq!(
        read_as(&workspace.host(), None, "AQWUserPref", "quality"),
        text("LOW")
    );
}

#[test]
fn each_account_keeps_its_own_saves() {
    let workspace = Workspace::new();
    saved_as(&workspace, Some("Hero"), "AQWUserPref", "quality", "LOW");
    let host = workspace.host();

    assert_eq!(
        read_as(&host, Some("  hero "), "AQWUserPref", "quality"),
        text("LOW")
    );
    assert_ne!(
        read_as(&host, Some("Sidekick"), "AQWUserPref", "quality"),
        text("LOW")
    );
    assert_ne!(read_as(&host, None, "AQWUserPref", "quality"), text("LOW"));
    assert!(
        workspace
            .path("saved-objects/accounts/hero/localhost/AQWUserPref.sol")
            .is_file()
    );
}

#[test]
fn sessions_of_one_account_share_saves() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let (_first, first) = testing::start_account_probe(&host, ProbeMovie::new(), "Hero");
    let (_second, second) = testing::start_account_probe(&host, ProbeMovie::new(), "Hero");

    save(&first, "AQLite_Data", "bHidePlayers", "yes");

    assert_eq!(read(&second, "AQLite_Data", "bHidePlayers"), text("yes"));
}

#[test]
fn imports_the_release_ruffle_web_saves_of_the_launched_account_once() {
    let written = produced("AQWUserPref", "quality", "LOW");
    let options = produced("AQLite_Data", "bHidePlayers", "yes");
    let workspace = Workspace::new();
    ruffle_web_saves(&workspace, HERO, &[("AQWUserPref", &written)], false);
    ruffle_web_saves(&workspace, HERO, &[("AQLite_Data", &options)], true);
    let before = snapshot(&local_storage(&workspace, HERO));

    let host = workspace.host();
    assert_eq!(
        read_as(&host, Some("Hero"), "AQWUserPref", "quality"),
        text("LOW")
    );
    assert_eq!(
        read_as(&host, Some("Hero"), "AQLite_Data", "bHidePlayers"),
        text("yes")
    );
    assert_eq!(
        fs::read(workspace.path("saved-objects/accounts/hero/localhost/AQWUserPref.sol")).unwrap(),
        written
    );
    assert_eq!(
        snapshot(&workspace.path("saved-objects/accounts/hero/localhost"))
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        ["AQLite_Data.sol", "AQWUserPref.sol"]
    );
    assert_eq!(snapshot(&local_storage(&workspace, HERO)), before);

    let (session, link) = testing::start_account_probe(&host, ProbeMovie::new(), "Hero");
    save(&link, "AQWUserPref", "quality", "HIGH");
    drop((link, session, host));
    assert_eq!(
        read_as(&workspace.host(), Some("Hero"), "AQWUserPref", "quality"),
        text("HIGH")
    );
}

#[test]
fn flash_era_saves_are_not_imported() {
    let written = produced("AQWUserPref", "quality", "LOW");
    let workspace = Workspace::new();
    let pepper = [
        "Partitions",
        HERO,
        "Pepper Data",
        "Shockwave Flash",
        "WritableRoot",
        "#SharedObjects",
        "N9QSFT4S",
        "game.aq.com",
    ]
    .iter()
    .fold(workspace.release(), |path, part| path.join(part));
    fs::create_dir_all(&pepper).unwrap();
    fs::write(pepper.join("AQWUserPref.sol"), &written).unwrap();

    assert_ne!(
        read_as(&workspace.host(), Some("Hero"), "AQWUserPref", "quality"),
        text("LOW")
    );
}

#[test]
fn saves_already_in_the_new_folder_are_not_replaced_by_the_release() {
    let written = produced("AQWUserPref", "quality", "LOW");
    let workspace = Workspace::new();
    saved_as(
        &workspace,
        Some("Hero"),
        "AQLite_Data",
        "bHidePlayers",
        "yes",
    );
    ruffle_web_saves(&workspace, HERO, &[("AQWUserPref", &written)], false);

    let host = workspace.host();
    assert_ne!(
        read_as(&host, Some("Hero"), "AQWUserPref", "quality"),
        text("LOW")
    );
    assert_eq!(
        read_as(&host, Some("Hero"), "AQLite_Data", "bHidePlayers"),
        text("yes")
    );
}

#[test]
fn an_account_name_no_folder_can_hold_keeps_saves_in_memory_only() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let (_session, link) = testing::start_account_probe(&host, ProbeMovie::new(), "con");

    save(&link, "AQWUserPref", "quality", "LOW");

    assert_eq!(read(&link, "AQWUserPref", "quality"), text("LOW"));
    assert!(!workspace.path("saved-objects/accounts").exists());
}

fn create(username: &str) -> AccountEdit {
    AccountEdit::CreateAccount {
        label: None,
        username: username.into(),
        password: Password::new("secret"),
    }
}

fn hold_app_data(host: &Host) -> mpsc::Sender<()> {
    let (release, gate) = mpsc::channel::<()>();
    let (entered, reached) = mpsc::channel::<()>();
    let gate = Mutex::new(Some((gate, entered)));
    host.set_app_data_probe(move || {
        if let Some((gate, entered)) = gate.lock().unwrap().take() {
            let _ = entered.send(());
            let _ = gate.recv_timeout(TIMEOUT);
        }
    });
    drop(host.accounts().edit(create("Holder")));
    reached.recv_timeout(TIMEOUT).unwrap();
    release
}

#[test]
fn a_session_that_only_read_a_save_does_not_undo_a_newer_one_when_it_closes() {
    let workspace = Workspace::new();
    saved_as(&workspace, Some("Hero"), "AQWUserPref", "quality", "LOW");
    let host = workspace.host();
    let (reader_session, reader) = testing::start_account_probe(&host, ProbeMovie::new(), "Hero");
    assert_eq!(read(&reader, "AQWUserPref", "quality"), text("LOW"));
    let (writer_session, writer) = testing::start_account_probe(&host, ProbeMovie::new(), "Hero");

    save(&writer, "AQWUserPref", "quality", "HIGH");
    drop((reader, reader_session));

    assert_eq!(
        read_as(&host, Some("Hero"), "AQWUserPref", "quality"),
        text("HIGH")
    );
    drop((writer, writer_session, host));
    assert_eq!(
        read_as(&workspace.host(), Some("Hero"), "AQWUserPref", "quality"),
        text("HIGH")
    );
}

#[test]
fn closing_everything_while_a_save_load_is_queued_keeps_later_writes() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let release = hold_app_data(&host);
    let (session, link) = testing::start_account_probe(&host, ProbeMovie::new(), "Hero");
    drop(host.accounts().edit(create("Later")));
    drop((link, session));

    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        let _ = release.send(());
    });
    drop(host);
    releaser.join().unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    let saved = loop {
        let accounts = fs::read_to_string(workspace.path("accounts.json")).unwrap_or_default();
        if accounts.contains("Later") || Instant::now() > deadline {
            break accounts;
        }
        thread::sleep(Duration::from_millis(20));
    };
    assert!(saved.contains("Later"), "{saved}");
}

#[test]
fn saves_that_load_late_are_never_overwritten_with_defaults() {
    let workspace = Workspace::new();
    saved_as(&workspace, Some("Hero"), "AQWUserPref", "quality", "LOW");
    saved_as(
        &workspace,
        Some("Hero"),
        "AQLite_Data",
        "bHidePlayers",
        "yes",
    );
    let host = workspace.host();
    let release = hold_app_data(&host);
    let (session, link) = testing::start_account_probe(&host, ProbeMovie::new(), "Hero");

    assert_ne!(read(&link, "AQWUserPref", "quality"), text("LOW"));
    save(&link, "AQWUserPref", "quality", "DEFAULT");
    release.send(()).unwrap();
    assert_eq!(
        wait(host.accounts().snapshot()).unwrap().accounts().len(),
        1
    );
    assert_eq!(read(&link, "AQLite_Data", "bHidePlayers"), text("yes"));
    drop((link, session, host));

    assert_eq!(
        read_as(&workspace.host(), Some("Hero"), "AQWUserPref", "quality"),
        text("LOW")
    );
}

#[cfg(unix)]
#[test]
fn an_unreadable_save_is_never_overwritten() {
    use std::os::unix::fs::PermissionsExt;
    if !common::app_data::permissions_apply() {
        return;
    }
    let workspace = Workspace::new();
    saved_as(&workspace, Some("Hero"), "AQWUserPref", "quality", "LOW");
    let file = workspace.path("saved-objects/accounts/hero/localhost/AQWUserPref.sol");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();

    let host = workspace.host();
    let (session, link) = testing::start_account_probe(&host, ProbeMovie::new(), "Hero");
    assert_ne!(read(&link, "AQWUserPref", "quality"), text("LOW"));
    save(&link, "AQWUserPref", "quality", "DEFAULT");
    save(&link, "AQLite_Data", "bHidePlayers", "yes");
    drop((link, session, host));
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();

    let host = workspace.host();
    assert_eq!(
        read_as(&host, Some("Hero"), "AQWUserPref", "quality"),
        text("LOW")
    );
    assert_eq!(
        read_as(&host, Some("Hero"), "AQLite_Data", "bHidePlayers"),
        text("yes")
    );
}
