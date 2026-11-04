mod common;

use common::app_data::*;
use lucent_host::accounts::AccountEdit;
use lucent_host::app_settings::PreferencesPatch;
use lucent_protocol::script_options::ScriptOptionsPatch;
use serde_json::{Value, json};
use std::fs;

const ACCOUNTS: &str = r#"{"accounts":[{"label":"Hero","username":"Hero","password":"secret"}],"groups":{"Farm":["Hero"]}}"#;

fn usernames(workspace: &Workspace) -> Vec<String> {
    let host = workspace.host();
    let snapshot = wait(host.accounts().snapshot()).unwrap();
    snapshot
        .accounts()
        .iter()
        .map(|account| account.username().to_owned())
        .collect()
}

fn imported(capture: &Capture) -> Vec<Value> {
    capture
        .take()
        .into_iter()
        .filter(|event| {
            event["message"]
                .as_str()
                .is_some_and(|text| text.starts_with("Imported"))
        })
        .collect()
}

#[test]
fn imports_release_accounts_once_and_never_writes_the_release_folder() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    workspace.write_release("accounts.json", ACCOUNTS);

    assert_eq!(usernames(&workspace), ["Hero"]);
    let saved: Value = serde_json::from_str(&workspace.read("accounts.json")).unwrap();
    assert_eq!(saved["accounts"][0]["username"], "Hero");
    assert_eq!(saved["groups"]["Farm"][0], "Hero");
    assert_eq!(
        imported(&capture),
        [json!({
            "level": "INFO",
            "message": "Imported accounts from the old data folder",
            "component": "accounts",
            "path": "accounts.json",
            "from": "release-app-data",
        })]
    );

    let host = workspace.host();
    wait(host.accounts().edit(AccountEdit::DeleteAccounts {
        usernames: vec!["Hero".into()],
    }))
    .unwrap();
    drop(host);
    assert_eq!(usernames(&workspace), Vec::<String>::new());
    assert_eq!(workspace.read_release("accounts.json"), ACCOUNTS);
    assert_eq!(imported(&capture), Vec::<Value>::new());
}

#[test]
fn starting_the_host_imports_every_release_document_without_any_read() {
    let workspace = Workspace::new();
    workspace.write_release("accounts.json", ACCOUNTS);
    workspace.write_release(
        "settings.json",
        r#"{"preferences":{"checkForUpdates":true}}"#,
    );
    workspace.write_release(
        "account-settings/hero.json",
        r#"{"scripts":{"restartAfterReconnect":true}}"#,
    );
    workspace.write_release(
        "account-settings/sidekick.json",
        r#"{"scripts":{"safeStartStop":true}}"#,
    );
    workspace.write_release("account-settings/notes.txt", "not a document");

    drop(workspace.host());

    for file in [
        "accounts.json",
        "settings.json",
        "account-settings/hero.json",
        "account-settings/sidekick.json",
    ] {
        assert!(workspace.path(file).is_file(), "{file} was not imported");
    }
    assert!(!workspace.path("account-settings/notes.txt").exists());
}

#[test]
fn a_document_already_in_the_new_folder_wins_over_the_release() {
    let workspace = Workspace::new();
    workspace.write_release("accounts.json", ACCOUNTS);
    workspace.write("accounts.json", r#"{"accounts":[]}"#);

    assert_eq!(usernames(&workspace), Vec::<String>::new());
    assert_eq!(workspace.read("accounts.json"), r#"{"accounts":[]}"#);
}

#[test]
fn an_unreadable_release_document_is_left_behind() {
    let workspace = Workspace::new();
    workspace.write_release("accounts.json", "{");

    assert_eq!(usernames(&workspace), Vec::<String>::new());
    assert!(!workspace.path("accounts.json").exists());
    assert_eq!(workspace.read_release("accounts.json"), "{");
}

#[test]
fn an_edit_before_any_read_starts_from_the_release_accounts() {
    let workspace = Workspace::new();
    workspace.write_release("accounts.json", ACCOUNTS);
    let host = workspace.host();

    let snapshot = wait(host.accounts().edit(AccountEdit::CreateGroup {
        name: "Solo".into(),
        usernames: vec!["Hero".into()],
    }))
    .unwrap();

    let groups: Vec<&str> = snapshot.groups().iter().map(|group| group.name()).collect();
    assert_eq!(groups, ["Farm", "Solo"]);
    assert_eq!(workspace.read_release("accounts.json"), ACCOUNTS);
}

#[test]
fn imports_account_settings_per_account() {
    let workspace = Workspace::new();
    workspace.write_release(
        "account-settings/hero.json",
        r#"{"scripts":{"restartAfterReconnect":true}}"#,
    );
    let host = workspace.host();

    let settings = wait(host.accounts().settings("Hero")).unwrap();
    assert!(settings.restart_after_reconnect);
    assert!(workspace.path("account-settings/hero.json").is_file());

    wait(host.accounts().update_settings(
        "Hero",
        ScriptOptionsPatch {
            restart_after_reconnect: Some(false),
            ..ScriptOptionsPatch::default()
        },
    ))
    .unwrap();
    assert!(
        !wait(host.accounts().settings("Hero"))
            .unwrap()
            .restart_after_reconnect
    );
    assert_eq!(
        workspace.read_release("account-settings/hero.json"),
        r#"{"scripts":{"restartAfterReconnect":true}}"#
    );
    assert!(
        !wait(host.accounts().settings("Other"))
            .unwrap()
            .restart_after_reconnect
    );
    assert!(!workspace.path("account-settings/other.json").exists());
}

#[test]
fn imports_app_settings_and_keeps_later_changes_in_the_new_folder() {
    let workspace = Workspace::new();
    let release = r#"{"preferences":{"checkForUpdates":true}}"#;
    workspace.write_release("settings.json", release);
    let host = workspace.host();

    assert!(
        wait(host.app_settings().get())
            .unwrap()
            .preferences
            .check_for_updates
    );
    wait(host.app_settings().update_preferences(PreferencesPatch {
        check_for_updates: Some(false),
        ..PreferencesPatch::default()
    }))
    .unwrap();
    drop(host);

    let host = workspace.host();
    assert!(
        !wait(host.app_settings().get())
            .unwrap()
            .preferences
            .check_for_updates
    );
    assert_eq!(workspace.read_release("settings.json"), release);
}

#[test]
fn the_release_update_cache_is_not_imported() {
    let workspace = Workspace::new();
    workspace.write_release(
        "release-cache.json",
        r#"{"release":{"version":"9.0.0","tagName":"v9.0.0","htmlUrl":"https://github.com/toommyliu/lucent/releases/tag/v9.0.0"},"skippedVersion":"9.0.0"}"#,
    );
    let host = workspace.host();

    assert!(wait(host.updates().should_prompt(&"9.0.0".parse().unwrap())).unwrap());
    assert!(fs::read_dir(workspace.app_data()).unwrap().next().is_none());
}
