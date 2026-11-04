use serde_json::{Map, Value, json};

use super::json;
use super::{Decoded, Issues, Loaded, decode_document, integer, newer_version, trim};
use crate::app_settings::{
    AppSettings, Appearance, Command, Font, FontSize, Hotkey, Hotkeys, LaunchMode, MotionMode,
    Preferences, Rgb, Rounding, ThemeMode, ThemeProfile, ThemeToken, ThemeTokens, ThemeVariant,
    Themes,
};

const VERSION: u32 = 1;

const LAUNCH_MODES: [(LaunchMode, &str); 2] = [
    (LaunchMode::Game, "game"),
    (LaunchMode::AccountManager, "account-manager"),
];
const THEME_MODES: [(ThemeMode, &str); 3] = [
    (ThemeMode::System, "system"),
    (ThemeMode::Light, "light"),
    (ThemeMode::Dark, "dark"),
];
const MOTION_MODES: [(MotionMode, &str); 3] = [
    (MotionMode::System, "system"),
    (MotionMode::On, "on"),
    (MotionMode::Off, "off"),
];

struct Paths {
    profile: &'static str,
    tokens: &'static str,
    sans_font: &'static str,
    mono_font: &'static str,
    sans_font_size: &'static str,
    mono_font_size: &'static str,
    rounding: &'static str,
}

macro_rules! paths {
    ($variant:literal) => {
        Paths {
            profile: concat!("appearance.themes.", $variant),
            tokens: concat!("appearance.themes.", $variant, ".tokens"),
            sans_font: concat!("appearance.themes.", $variant, ".sansFont"),
            mono_font: concat!("appearance.themes.", $variant, ".monoFont"),
            sans_font_size: concat!("appearance.themes.", $variant, ".sansFontSize"),
            mono_font_size: concat!("appearance.themes.", $variant, ".monoFontSize"),
            rounding: concat!("appearance.themes.", $variant, ".rounding"),
        }
    };
}

const LIGHT: Paths = paths!("light");
const DARK: Paths = paths!("dark");

pub(in crate::data) fn decode(file: Option<&[u8]>) -> Loaded<AppSettings> {
    decode_document(file, from_object)
}

pub(in crate::data) fn encode(settings: &AppSettings) -> Vec<u8> {
    let preferences = &settings.preferences;
    let appearance = &settings.appearance;
    let bindings: Vec<Value> = Command::ALL
        .into_iter()
        .map(|command| json!({"id": command.id(), "value": settings.hotkeys.get(command).as_str()}))
        .collect();
    json::stringify(&json!({
        "version": VERSION,
        "preferences": {
            "checkForUpdates": preferences.check_for_updates,
            "launchMode": name(&LAUNCH_MODES, preferences.launch_mode),
            "showGameUsernameInWindowTitle": preferences.show_game_username_in_window_title,
            "useGameTabs": preferences.use_game_tabs,
        },
        "appearance": {
            "themeMode": name(&THEME_MODES, appearance.theme_mode),
            "reduceMotion": name(&MOTION_MODES, appearance.reduce_motion),
            "useCursorPointers": appearance.use_cursor_pointers,
            "themes": {
                "light": profile_value(&appearance.themes.light),
                "dark": profile_value(&appearance.themes.dark),
            },
        },
        "hotkeys": {"bindings": bindings},
    }))
}

fn profile_value(profile: &ThemeProfile) -> Value {
    let tokens: Map<String, Value> = ThemeToken::ALL
        .into_iter()
        .map(|token| {
            let [red, green, blue] = profile.tokens[token];
            (
                token.name().to_owned(),
                format!("#{red:02x}{green:02x}{blue:02x}").into(),
            )
        })
        .collect();
    let rounding = profile.rounding.get();
    json!({
        "tokens": tokens,
        "sansFont": profile.sans_font.as_str(),
        "monoFont": profile.mono_font.as_str(),
        "sansFontSize": profile.sans_font_size.get(),
        "monoFontSize": profile.mono_font_size.get(),
        "rounding": if rounding.fract() == 0.0 { Value::from(rounding as u8) } else { Value::from(rounding) },
    })
}

fn name<T: PartialEq + Copy>(names: &[(T, &'static str)], value: T) -> &'static str {
    names.iter().find(|(known, _)| *known == value).unwrap().1
}

fn from_object(document: &Map<String, Value>) -> Decoded<AppSettings> {
    let mut issues = Issues::default();
    let newer = newer_version(document, VERSION, &mut issues);
    let reset = &mut issues.reset;
    let defaults = AppSettings::default();
    let preferences = object(document.get("preferences"), "preferences", reset);
    let preferences = Preferences {
        check_for_updates: boolean(
            field(preferences, "checkForUpdates"),
            "preferences.checkForUpdates",
            defaults.preferences.check_for_updates,
            reset,
        ),
        launch_mode: choice(
            field(preferences, "launchMode"),
            "preferences.launchMode",
            &LAUNCH_MODES,
            defaults.preferences.launch_mode,
            reset,
        ),
        show_game_username_in_window_title: boolean(
            field(preferences, "showGameUsernameInWindowTitle"),
            "preferences.showGameUsernameInWindowTitle",
            defaults.preferences.show_game_username_in_window_title,
            reset,
        ),
        use_game_tabs: boolean(
            field(preferences, "useGameTabs"),
            "preferences.useGameTabs",
            defaults.preferences.use_game_tabs,
            reset,
        ),
    };
    let appearance = object(document.get("appearance"), "appearance", reset);
    let theme_mode = choice(
        field(appearance, "themeMode"),
        "appearance.themeMode",
        &THEME_MODES,
        defaults.appearance.theme_mode,
        reset,
    );
    let reduce_motion = choice(
        field(appearance, "reduceMotion"),
        "appearance.reduceMotion",
        &MOTION_MODES,
        defaults.appearance.reduce_motion,
        reset,
    );
    let use_cursor_pointers = boolean(
        field(appearance, "useCursorPointers"),
        "appearance.useCursorPointers",
        defaults.appearance.use_cursor_pointers,
        reset,
    );
    let themes = object(field(appearance, "themes"), "appearance.themes", reset);
    let themes = Themes {
        light: profile(field(themes, "light"), ThemeVariant::Light, &LIGHT, reset),
        dark: profile(field(themes, "dark"), ThemeVariant::Dark, &DARK, reset),
    };
    let hotkeys = hotkeys(document.get("hotkeys"), reset);
    Decoded {
        value: AppSettings {
            preferences,
            appearance: Appearance {
                theme_mode,
                reduce_motion,
                use_cursor_pointers,
                themes,
            },
            hotkeys,
        },
        issues: if newer { Issues::default() } else { issues },
    }
}

fn field<'a>(section: Option<&'a Map<String, Value>>, name: &str) -> Option<&'a Value> {
    section.and_then(|section| section.get(name))
}

fn object<'a>(
    raw: Option<&'a Value>,
    path: &'static str,
    reset: &mut Vec<&'static str>,
) -> Option<&'a Map<String, Value>> {
    match raw {
        Some(Value::Object(fields)) => Some(fields),
        Some(_) => {
            reset.push(path);
            None
        }
        None => None,
    }
}

fn leaf<T>(
    raw: Option<&Value>,
    path: &'static str,
    fallback: T,
    reset: &mut Vec<&'static str>,
    read: impl Fn(&Value) -> Option<T>,
    same: impl Fn(&Value, &T) -> bool,
) -> T {
    let Some(raw) = raw else {
        return fallback;
    };
    let value = read(raw).unwrap_or(fallback);
    if !same(raw, &value) {
        reset.push(path);
    }
    value
}

fn boolean(
    raw: Option<&Value>,
    path: &'static str,
    fallback: bool,
    reset: &mut Vec<&'static str>,
) -> bool {
    leaf(raw, path, fallback, reset, Value::as_bool, |raw, value| {
        raw.as_bool() == Some(*value)
    })
}

fn choice<T: PartialEq + Copy>(
    raw: Option<&Value>,
    path: &'static str,
    names: &[(T, &'static str)],
    fallback: T,
    reset: &mut Vec<&'static str>,
) -> T {
    let read = |raw: &Value| {
        let text = raw.as_str()?;
        let (value, _) = names.iter().find(|(_, name)| *name == text)?;
        Some(*value)
    };
    leaf(raw, path, fallback, reset, read, |raw, value| {
        read(raw) == Some(*value)
    })
}

fn same_number(raw: &Value, value: f64) -> bool {
    raw.as_f64().map(f64::to_bits) == Some(value.to_bits())
}

fn profile(
    raw: Option<&Value>,
    variant: ThemeVariant,
    paths: &Paths,
    reset: &mut Vec<&'static str>,
) -> ThemeProfile {
    let fallback = ThemeProfile::default_for(variant);
    let Some(profile) = object(raw, paths.profile, reset) else {
        return fallback;
    };
    let font = |raw: Option<&Value>, path, fallback: Font, reset: &mut Vec<&'static str>| {
        let read = |raw: &Value| raw.as_str().and_then(Font::new);
        let same = |raw: &Value, font: &Font| raw.as_str().map(trim) == Some(font.as_str());
        leaf(raw, path, fallback, reset, read, same)
    };
    let size = |raw: Option<&Value>, path, fallback: FontSize, reset: &mut Vec<&'static str>| {
        let read = |raw: &Value| raw.as_f64().map(FontSize::rounded);
        let same = |raw: &Value, size: &FontSize| same_number(raw, size.get().into());
        leaf(raw, path, fallback, reset, read, same)
    };
    ThemeProfile {
        tokens: tokens(profile.get("tokens"), fallback.tokens, paths.tokens, reset),
        sans_font: font(
            profile.get("sansFont"),
            paths.sans_font,
            fallback.sans_font,
            reset,
        ),
        mono_font: font(
            profile.get("monoFont"),
            paths.mono_font,
            fallback.mono_font,
            reset,
        ),
        sans_font_size: size(
            profile.get("sansFontSize"),
            paths.sans_font_size,
            fallback.sans_font_size,
            reset,
        ),
        mono_font_size: size(
            profile.get("monoFontSize"),
            paths.mono_font_size,
            fallback.mono_font_size,
            reset,
        ),
        rounding: leaf(
            profile.get("rounding"),
            paths.rounding,
            fallback.rounding,
            reset,
            |raw| raw.as_f64().map(Rounding::clamped),
            |raw, rounding| same_number(raw, rounding.get()),
        ),
    }
}

fn tokens(
    raw: Option<&Value>,
    mut tokens: ThemeTokens,
    path: &'static str,
    reset: &mut Vec<&'static str>,
) -> ThemeTokens {
    let Some(raw) = object(raw, path, reset) else {
        return tokens;
    };
    let mut invalid = false;
    for (name, value) in raw {
        let token = ThemeToken::ALL
            .into_iter()
            .find(|token| token.name() == name);
        match (token, rgb(value)) {
            (Some(token), Some(rgb)) => tokens.set(token, rgb),
            _ => invalid = true,
        }
    }
    if invalid {
        reset.push(path);
    }
    tokens
}

fn rgb(raw: &Value) -> Option<Rgb> {
    match raw {
        Value::String(text) => {
            let text = trim(text);
            let hex = text.strip_prefix('#').unwrap_or(text);
            if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap();
            Some([channel(0), channel(2), channel(4)])
        }
        Value::Array(channels) if channels.len() == 3 => {
            let channel = |raw: &Value| {
                let value = integer(raw)?;
                (0.0..=255.0).contains(&value).then_some(value as u8)
            };
            Some([
                channel(&channels[0])?,
                channel(&channels[1])?,
                channel(&channels[2])?,
            ])
        }
        _ => None,
    }
}

fn binding(raw: &Value) -> Option<(&str, &str)> {
    let binding = raw.as_object()?;
    Some((
        binding.get("id")?.as_str()?,
        binding.get("value")?.as_str()?,
    ))
}

fn hotkeys(raw: Option<&Value>, reset: &mut Vec<&'static str>) -> Hotkeys {
    let mut hotkeys = Hotkeys::default();
    let Some(Value::Object(saved)) = raw else {
        if raw.is_some() {
            reset.push("hotkeys");
        }
        return hotkeys;
    };
    let Some(bindings) = saved.get("bindings") else {
        return hotkeys;
    };
    let bindings: Option<Vec<_>> = match bindings {
        Value::Array(bindings) => bindings.iter().map(binding).collect(),
        _ => None,
    };
    let Some(bindings) = bindings else {
        reset.push("hotkeys.bindings");
        return hotkeys;
    };
    let mut invalid = false;
    let mut seen = std::collections::HashSet::new();
    for (id, value) in bindings {
        invalid |= !seen.insert(id);
        match (Command::from_id(id), Hotkey::parse(value)) {
            (Some(command), Some(hotkey)) => hotkeys.set(command, hotkey),
            _ => invalid = true,
        }
    }
    if invalid {
        reset.push("hotkeys.bindings");
    }
    hotkeys
}
