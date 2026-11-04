mod common;

use common::app_data::*;
use lucent_host::Host;
use lucent_host::app_settings::{
    AppSettings, AppearancePatch, Command, Font, FontSize, Hotkey, LaunchMode, MotionMode,
    PreferencesPatch, Rounding, SettingsError, ThemeMode, ThemePatch, ThemeToken,
};
use serde_json::{Value, json};
use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::task::Waker;
use std::thread;

const UNREADABLE: &str = "Saved settings could not be read; using defaults until the next save";
const INVALID: &str = "Saved settings had invalid entries; ignoring them until the next save";

fn warning(message: &str, fields: Value) -> Value {
    let mut event = json!({
        "level": "WARN",
        "message": message,
        "component": "settings",
        "path": "settings.json",
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    event
}

fn saved(workspace: &Workspace) -> Value {
    serde_json::from_str(&workspace.read("settings.json")).unwrap()
}

fn watch(host: &Host) -> Arc<Wakes> {
    let wakes = Arc::new(Wakes::default());
    host.app_settings().watch(Waker::from(wakes.clone()));
    wakes
}

fn get(host: &Host) -> AppSettings {
    wait(host.app_settings().get()).unwrap()
}

fn light() -> AppearancePatch {
    AppearancePatch {
        theme_mode: Some(ThemeMode::Light),
        ..Default::default()
    }
}

fn check_for_updates() -> PreferencesPatch {
    PreferencesPatch {
        check_for_updates: Some(true),
        ..Default::default()
    }
}

fn hotkey(text: &str) -> Option<Hotkey> {
    Some(Hotkey::parse(text).unwrap())
}

#[test]
fn persists_an_update_and_wakes_watchers_once() {
    let workspace = Workspace::new();
    let host = workspace.host();
    get(&host);
    let wakes = watch(&host);
    let updated = wait(host.app_settings().update_appearance(light())).unwrap();
    assert_eq!(updated.appearance.theme_mode, ThemeMode::Light);
    let file = saved(&workspace);
    assert_eq!(file["appearance"]["themeMode"], "light");
    assert_eq!(file["hotkeys"]["bindings"].as_array().unwrap().len(), 23);
    assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
}

#[test]
fn an_update_that_changes_nothing_neither_writes_nor_wakes() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let writes = count_writes(&host);
    let wakes = watch(&host);
    let defaults = AppearancePatch {
        theme_mode: Some(ThemeMode::Dark),
        ..Default::default()
    };
    assert_eq!(
        wait(host.app_settings().update_appearance(defaults)).unwrap(),
        AppSettings::default()
    );
    wait(host.app_settings().reset_hotkeys()).unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(wakes.0.load(Ordering::SeqCst), 0);
    assert!(!workspace.path("settings.json").exists());
    wait(host.app_settings().update_appearance(light())).unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
}

#[test]
fn settings_changes_do_not_wake_account_watchers() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let accounts = Arc::new(Wakes::default());
    host.accounts().watch(Waker::from(accounts.clone()));
    wait(host.app_settings().update_appearance(light())).unwrap();
    assert_eq!(accounts.0.load(Ordering::SeqCst), 0);
}

#[test]
fn rejects_a_hotkey_already_assigned_without_saving() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let tabs = PreferencesPatch {
        use_game_tabs: Some(true),
        ..Default::default()
    };
    wait(host.app_settings().update_preferences(tabs)).unwrap();
    let before = workspace.read("settings.json");
    let taken = wait(
        host.app_settings()
            .update_hotkeys(vec![(Command::ToggleBank, hotkey("Mod+Shift+X"))]),
    );
    assert_eq!(taken, Err(SettingsError::HotkeyTaken(Command::ToggleBank)));
    assert_eq!(
        taken.unwrap_err().to_string(),
        "Hotkey is already assigned to Toggle Bank."
    );
    assert_eq!(workspace.read("settings.json"), before);
    assert_eq!(
        get(&host).hotkeys.get(Command::ToggleBank).as_str(),
        "Mod+B"
    );
}

#[test]
fn hotkey_updates_bind_unbind_and_restore_defaults() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let settings = wait(host.app_settings().update_hotkeys(vec![
        (Command::ToggleBank, hotkey("alt+shift+b")),
        (Command::OpenPackets, hotkey("ctrl p")),
        (Command::LoadScript, hotkey("")),
    ]))
    .unwrap();
    assert_eq!(
        settings.hotkeys.get(Command::ToggleBank).as_str(),
        "Alt+Shift+B"
    );
    assert_eq!(
        settings.hotkeys.get(Command::OpenPackets).as_str(),
        "Control+P"
    );
    assert!(settings.hotkeys.get(Command::LoadScript).is_unbound());
    let settings = wait(
        host.app_settings()
            .update_hotkeys(vec![(Command::ToggleBank, None)]),
    )
    .unwrap();
    assert_eq!(settings.hotkeys.get(Command::ToggleBank).as_str(), "Mod+B");
    let bindings = &saved(&workspace)["hotkeys"]["bindings"];
    assert_eq!(bindings[1], json!({"id": "loadScript", "value": ""}));
    assert_eq!(
        bindings[8],
        json!({"id": "openPackets", "value": "Control+P"})
    );
    assert_eq!(bindings[11], json!({"id": "toggleBank", "value": "Mod+B"}));
}

#[test]
fn resetting_appearance_keeps_hotkeys_and_preferences() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let settings = host.app_settings();
    wait(settings.update_preferences(check_for_updates())).unwrap();
    wait(settings.update_hotkeys(vec![(Command::ToggleBank, hotkey("Alt+Z"))])).unwrap();
    wait(settings.update_appearance(AppearancePatch {
        theme_mode: Some(ThemeMode::Light),
        reduce_motion: Some(MotionMode::Off),
        dark: ThemePatch {
            rounding: Rounding::new(0.5),
            ..Default::default()
        },
        ..Default::default()
    }))
    .unwrap();
    let reset = wait(settings.reset_appearance()).unwrap();
    assert_eq!(reset.appearance, AppSettings::default().appearance);
    assert!(reset.preferences.check_for_updates);
    assert_eq!(reset.hotkeys.get(Command::ToggleBank).as_str(), "Alt+Z");
    assert_eq!(
        saved(&workspace)["hotkeys"]["bindings"][11]["value"],
        "Alt+Z"
    );
}

#[test]
fn resetting_hotkeys_keeps_appearance_and_preferences() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let settings = host.app_settings();
    wait(settings.update_preferences(check_for_updates())).unwrap();
    wait(settings.update_appearance(light())).unwrap();
    wait(settings.update_hotkeys(vec![(Command::ToggleBank, hotkey("Alt+Z"))])).unwrap();
    let reset = wait(settings.reset_hotkeys()).unwrap();
    assert_eq!(reset.hotkeys, AppSettings::default().hotkeys);
    assert_eq!(reset.appearance.theme_mode, ThemeMode::Light);
    assert!(reset.preferences.check_for_updates);
    assert_eq!(saved(&workspace)["appearance"]["themeMode"], "light");
}

#[test]
fn theme_updates_change_one_variant_and_restore_default_tokens() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let settings = host.app_settings();
    wait(settings.update_appearance(AppearancePatch {
        dark: ThemePatch {
            tokens: vec![(ThemeToken::Background, Some([18, 52, 86]))],
            sans_font: Font::new("  Arial  "),
            mono_font_size: FontSize::new(20),
            ..Default::default()
        },
        ..Default::default()
    }))
    .unwrap();
    let file = saved(&workspace);
    let dark = &file["appearance"]["themes"]["dark"];
    assert_eq!(dark["tokens"]["background"], "#123456");
    assert_eq!(dark["sansFont"], "Arial");
    assert_eq!(dark["monoFontSize"], 20);
    assert_eq!(
        file["appearance"]["themes"]["light"]["tokens"]["background"],
        "#ffffff"
    );
    let restored = wait(settings.update_appearance(AppearancePatch {
        dark: ThemePatch {
            tokens: vec![(ThemeToken::Background, None)],
            ..Default::default()
        },
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        restored.appearance.themes.dark.tokens[ThemeToken::Background],
        [14, 14, 15]
    );
}

#[test]
fn concurrent_updates_before_the_first_load_all_survive() {
    let workspace = Workspace::new();
    let host = workspace.host();
    let appearance = {
        let host = host.clone();
        thread::spawn(move || wait(host.app_settings().update_appearance(light())).unwrap())
    };
    let preferences = {
        let host = host.clone();
        thread::spawn(move || {
            let patch = PreferencesPatch {
                launch_mode: Some(LaunchMode::AccountManager),
                show_game_username_in_window_title: Some(true),
                use_game_tabs: Some(true),
                ..Default::default()
            };
            wait(host.app_settings().update_preferences(patch)).unwrap()
        })
    };
    let readers: Vec<_> = (0..2)
        .map(|_| {
            let host = host.clone();
            thread::spawn(move || get(&host))
        })
        .collect();
    appearance.join().unwrap();
    preferences.join().unwrap();
    for reader in readers {
        reader.join().unwrap();
    }
    let current = get(&host);
    assert_eq!(current.appearance.theme_mode, ThemeMode::Light);
    assert_eq!(current.preferences.launch_mode, LaunchMode::AccountManager);
    assert!(current.preferences.use_game_tabs);
    assert!(current.preferences.show_game_username_in_window_title);
    let file = saved(&workspace);
    assert_eq!(file["appearance"]["themeMode"], "light");
    assert_eq!(file["preferences"]["launchMode"], "account-manager");
    assert_eq!(file["preferences"]["useGameTabs"], true);
}

#[test]
fn saves_without_recovery_warnings_before_loading() {
    for source in ["{", r#"{"appearance":{"themeMode":"invalid"}}"#] {
        let (capture, _guard) = Capture::start();
        let workspace = Workspace::new();
        let host = workspace.host();
        workspace.write("settings.json", source);
        let updated = wait(host.app_settings().update_appearance(light())).unwrap();
        assert_eq!(updated.appearance.theme_mode, ThemeMode::Light);
        assert_eq!(saved(&workspace)["appearance"]["themeMode"], "light");
        assert_eq!(capture.take(), Vec::<Value>::new(), "{source}");
    }
}

#[test]
fn keeps_valid_settings_and_preserves_invalid_bytes_until_an_edit() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = r#"{
   "preferences": {
      "useGameTabs": true,
      "launchMode": "invalid"
   },
   "appearance": {
      "themeMode": "light",
      "reduceMotion": false
   }
}
"#;
    workspace.write("settings.json", source);
    let loaded = get(&host);
    assert!(loaded.preferences.use_game_tabs);
    assert_eq!(loaded.preferences.launch_mode, LaunchMode::Game);
    assert_eq!(loaded.appearance.theme_mode, ThemeMode::Light);
    assert_eq!(loaded.appearance.reduce_motion, MotionMode::System);
    assert_eq!(workspace.read("settings.json"), source);
    assert_eq!(
        capture.take(),
        [warning(
            INVALID,
            json!({"reset": r#"["preferences.launchMode", "appearance.reduceMotion"]"#}),
        )]
    );
    wait(host.app_settings().update_preferences(check_for_updates())).unwrap();
    assert_eq!(
        saved(&workspace)["preferences"],
        json!({
            "checkForUpdates": true,
            "launchMode": "game",
            "showGameUsernameInWindowTitle": false,
            "useGameTabs": true,
        })
    );
}

#[test]
fn reports_invalid_appearance_and_hotkeys_in_one_warning() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = json!({
        "appearance": {
            "themes": {
                "dark": {
                    "sansFont": "  ",
                    "rounding": "bad",
                    "tokens": {"background": "#123456", "private-token-name": "private"},
                },
            },
        },
        "hotkeys": {"bindings": [{"id": "toggleBank", "value": "Alt+Z"}, {"id": "loadScript", "value": 42}]},
    })
    .to_string();
    workspace.write("settings.json", &source);
    let loaded = get(&host);
    assert_eq!(
        loaded.appearance.themes.dark.tokens[ThemeToken::Background],
        [18, 52, 86]
    );
    assert_eq!(loaded.hotkeys.get(Command::ToggleBank).as_str(), "Mod+B");
    let events = capture.take();
    assert_eq!(
        events,
        [warning(
            INVALID,
            json!({"reset": r#"["appearance.themes.dark.tokens", "appearance.themes.dark.sansFont", "appearance.themes.dark.rounding", "hotkeys.bindings"]"#}),
        )]
    );
    assert!(!events[0].to_string().contains("private"));
    assert_eq!(workspace.read("settings.json"), source);
}

#[test]
fn uses_defaults_without_overwriting_unreadable_settings() {
    for (source, position) in [
        ("{invalid secret", json!({"line": 1, "column": 2})),
        ("[]", json!({})),
        ("null", json!({})),
    ] {
        let (capture, _guard) = Capture::start();
        let workspace = Workspace::new();
        let host = workspace.host();
        workspace.write("settings.json", source);
        assert_eq!(get(&host), AppSettings::default());
        assert_eq!(workspace.read("settings.json"), source);
        let mut fields = json!({"ignored": r#"{"file":1}"#});
        fields
            .as_object_mut()
            .unwrap()
            .extend(position.as_object().unwrap().clone());
        assert_eq!(capture.take(), [warning(UNREADABLE, fields)], "{source}");
    }
}

#[cfg(unix)]
#[test]
fn fails_reads_and_updates_without_replacing_unreadable_settings() {
    if !permissions_apply() {
        return;
    }
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = r#"{"version":1,"preferences":{"useGameTabs":true,"launchMode":"account-manager"},"appearance":{"themeMode":"light"}}
"#;
    workspace.write("settings.json", source);
    set_mode(&workspace.path("settings.json"), 0o000);
    let failure = format!(
        "JSON file read failed at {}.",
        workspace.path("settings.json").display()
    );
    let read = wait(host.app_settings().get()).unwrap_err();
    assert!(matches!(read, SettingsError::File(_)));
    assert_eq!(read.to_string(), failure);
    let update = wait(host.app_settings().update_preferences(check_for_updates()));
    assert_eq!(update.unwrap_err().to_string(), failure);
    assert_eq!(capture.take(), Vec::<Value>::new());
    set_mode(&workspace.path("settings.json"), 0o600);
    assert_eq!(workspace.read("settings.json"), source);
    let loaded = get(&host);
    assert!(loaded.preferences.use_game_tabs);
    assert_eq!(loaded.preferences.launch_mode, LaunchMode::AccountManager);
    assert_eq!(loaded.appearance.theme_mode, ThemeMode::Light);
}

#[test]
fn loads_missing_and_valid_partial_settings_silently_without_writing() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    assert_eq!(get(&host), AppSettings::default());
    assert!(!workspace.path("settings.json").exists());

    let workspace = Workspace::new();
    let host = workspace.host();
    let source = json!({
        "version": 0,
        "appearance": {
            "themeMode": "light",
            "themes": {"dark": {"sansFont": "  Arial  ", "tokens": {"background": "  123456  "}}},
        },
        "hotkeys": {"bindings": [{"id": "toggleBank", "value": "alt+z"}]},
    })
    .to_string();
    workspace.write("settings.json", &source);
    let loaded = get(&host);
    assert_eq!(loaded.appearance.themes.dark.sans_font.as_str(), "Arial");
    assert_eq!(
        loaded.appearance.themes.dark.tokens[ThemeToken::Background],
        [18, 52, 86]
    );
    assert_eq!(loaded.hotkeys.get(Command::ToggleBank).as_str(), "Alt+Z");
    assert_eq!(capture.take(), Vec::<Value>::new());
    assert_eq!(workspace.read("settings.json"), source);
}

#[test]
fn reads_newer_settings_silently_and_writes_the_current_version_on_update() {
    let (capture, _guard) = Capture::start();
    let workspace = Workspace::new();
    let host = workspace.host();
    let source = json!({
        "version": 2,
        "preferences": {"useGameTabs": true, "launchMode": "later"},
        "futureOnly": "retain",
    })
    .to_string();
    workspace.write("settings.json", &source);
    assert!(get(&host).preferences.use_game_tabs);
    assert_eq!(capture.take(), Vec::<Value>::new());
    assert_eq!(workspace.read("settings.json"), source);
    let tabs_off = PreferencesPatch {
        use_game_tabs: Some(false),
        ..Default::default()
    };
    let updated = wait(host.app_settings().update_preferences(tabs_off)).unwrap();
    assert!(!updated.preferences.use_game_tabs);
    let file = saved(&workspace);
    assert_eq!(file["version"], 1);
    assert_eq!(file["preferences"]["useGameTabs"], false);
    assert_eq!(file.get("futureOnly"), None);
}

#[test]
fn settings_files_are_owner_only() {
    if !permissions_apply() {
        return;
    }
    let workspace = Workspace::new();
    let host = workspace.host();
    wait(host.app_settings().update_appearance(light())).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(workspace.path("settings.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
