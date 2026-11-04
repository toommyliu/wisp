use std::collections::HashSet;
use std::ops::Index;
use std::sync::Arc;
use std::task::Waker;

use crate::accounts::FileError;
use crate::data::app_data::{AppData, Request};
use crate::data::documents::{is_space, trim};
use crate::reply::{Answer, Reply, Stopped, reply};

#[derive(Clone)]
pub struct AppSettingsStore {
    pub(crate) app_data: Arc<AppData>,
}

impl AppSettingsStore {
    pub fn get(&self) -> Reply<AppSettings, SettingsError> {
        self.ask(Request::AppSettings)
    }

    pub fn update_preferences(&self, patch: PreferencesPatch) -> Reply<AppSettings, SettingsError> {
        self.update(Update::Preferences(patch))
    }

    pub fn update_appearance(&self, patch: AppearancePatch) -> Reply<AppSettings, SettingsError> {
        self.update(Update::Appearance(patch))
    }

    pub fn update_hotkeys(
        &self,
        bindings: Vec<(Command, Option<Hotkey>)>,
    ) -> Reply<AppSettings, SettingsError> {
        self.update(Update::Hotkeys(bindings))
    }

    pub fn reset_appearance(&self) -> Reply<AppSettings, SettingsError> {
        self.update(Update::ResetAppearance)
    }

    pub fn reset_hotkeys(&self) -> Reply<AppSettings, SettingsError> {
        self.update(Update::ResetHotkeys)
    }

    pub fn watch(&self, waker: Waker) {
        self.app_data.settings_changes.watch(waker);
    }

    fn update(&self, update: Update) -> Reply<AppSettings, SettingsError> {
        self.ask(|answer| Request::UpdateAppSettings(update, answer))
    }

    fn ask<T>(
        &self,
        request: impl FnOnce(Answer<T, SettingsError>) -> Request,
    ) -> Reply<T, SettingsError> {
        let (answer, reply) = reply();
        self.app_data.submit(request(answer));
        reply
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("Hotkey is already assigned to {}.", .0.label())]
    HotkeyTaken(Command),
    #[error("{0}")]
    File(#[from] FileError),
    #[error("the app data service has stopped")]
    Stopped,
}

impl Stopped for SettingsError {
    const STOPPED: Self = SettingsError::Stopped;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppSettings {
    pub preferences: Preferences,
    pub appearance: Appearance,
    pub hotkeys: Hotkeys,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    pub check_for_updates: bool,
    pub launch_mode: LaunchMode,
    pub show_game_username_in_window_title: bool,
    pub use_game_tabs: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaunchMode {
    #[default]
    Game,
    AccountManager,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Appearance {
    pub theme_mode: ThemeMode,
    pub reduce_motion: MotionMode,
    pub use_cursor_pointers: bool,
    pub themes: Themes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    #[default]
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MotionMode {
    #[default]
    System,
    On,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeVariant {
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Themes {
    pub light: ThemeProfile,
    pub dark: ThemeProfile,
}

impl Default for Themes {
    fn default() -> Self {
        Self {
            light: ThemeProfile::default_for(ThemeVariant::Light),
            dark: ThemeProfile::default_for(ThemeVariant::Dark),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeProfile {
    pub tokens: ThemeTokens,
    pub sans_font: Font,
    pub mono_font: Font,
    pub sans_font_size: FontSize,
    pub mono_font_size: FontSize,
    pub rounding: Rounding,
}

impl ThemeProfile {
    pub fn default_for(variant: ThemeVariant) -> Self {
        Self {
            tokens: ThemeTokens::default_for(variant),
            sans_font: Font(DEFAULT_SANS_FONT.into()),
            mono_font: Font(DEFAULT_MONO_FONT.into()),
            sans_font_size: FontSize(14),
            mono_font_size: FontSize(12),
            rounding: Rounding(1.0),
        }
    }
}

const DEFAULT_SANS_FONT: &str = r#"-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif"#;
const DEFAULT_MONO_FONT: &str =
    r#"ui-monospace, "SFMono-Regular", "SF Mono", Menlo, Consolas, "Liberation Mono", monospace"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font(String);

impl Font {
    pub fn new(text: &str) -> Option<Self> {
        let text = trim(text);
        (1..=256)
            .contains(&text.encode_utf16().count())
            .then(|| Self(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontSize(u8);

impl FontSize {
    const RANGE: std::ops::RangeInclusive<u8> = 10..=24;

    pub fn new(size: u8) -> Option<Self> {
        Self::RANGE.contains(&size).then_some(Self(size))
    }

    pub(crate) fn rounded(size: f64) -> Self {
        let floor = size.floor();
        let rounded = if size - floor >= 0.5 {
            floor + 1.0
        } else {
            floor
        };
        let (min, max) = Self::RANGE.into_inner();
        Self(rounded.clamp(f64::from(min), f64::from(max)) as u8)
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rounding(f64);

impl Rounding {
    pub fn new(rounding: f64) -> Option<Self> {
        (0.0..=2.0)
            .contains(&rounding)
            .then(|| Self::clamped(rounding))
    }

    pub(crate) fn clamped(rounding: f64) -> Self {
        Self(if rounding > 0.0 {
            rounding.min(2.0)
        } else {
            0.0
        })
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

pub type Rgb = [u8; 3];

macro_rules! theme_tokens {
    ($($token:ident $name:literal $light:expr, $dark:expr;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum ThemeToken {
            $($token,)*
        }

        impl ThemeToken {
            pub const ALL: [ThemeToken; 25] = [$(ThemeToken::$token,)*];

            pub fn name(self) -> &'static str {
                match self {
                    $(ThemeToken::$token => $name,)*
                }
            }
        }

        const LIGHT_TOKENS: [Rgb; 25] = [$($light,)*];
        const DARK_TOKENS: [Rgb; 25] = [$($dark,)*];
    };
}

theme_tokens! {
    Background "background" [255, 255, 255], [14, 14, 15];
    Foreground "foreground" [38, 38, 38], [245, 245, 245];
    Card "card" [255, 255, 255], [18, 18, 20];
    CardForeground "cardForeground" [38, 38, 38], [245, 245, 245];
    Popover "popover" [255, 255, 255], [22, 22, 24];
    PopoverForeground "popoverForeground" [38, 38, 38], [245, 245, 245];
    Primary "primary" [38, 38, 38], [245, 245, 245];
    PrimaryForeground "primaryForeground" [250, 250, 250], [38, 38, 38];
    Secondary "secondary" [245, 245, 245], [32, 32, 34];
    SecondaryForeground "secondaryForeground" [38, 38, 38], [245, 245, 245];
    Muted "muted" [245, 245, 245], [32, 32, 34];
    MutedForeground "mutedForeground" [92, 92, 92], [166, 166, 166];
    Accent "accent" [245, 245, 245], [32, 32, 34];
    AccentForeground "accentForeground" [38, 38, 38], [245, 245, 245];
    Destructive "destructive" [239, 68, 68], [248, 113, 113];
    DestructiveForeground "destructiveForeground" [185, 28, 28], [248, 113, 113];
    Success "success" [16, 185, 129], [52, 211, 153];
    SuccessForeground "successForeground" [4, 120, 87], [52, 211, 153];
    Warning "warning" [245, 158, 11], [251, 191, 36];
    WarningForeground "warningForeground" [180, 83, 9], [251, 191, 36];
    Info "info" [59, 130, 246], [96, 165, 250];
    InfoForeground "infoForeground" [29, 78, 216], [96, 165, 250];
    Border "border" [235, 235, 235], [38, 38, 40];
    Input "input" [229, 229, 229], [46, 46, 49];
    Ring "ring" [163, 163, 163], [115, 115, 115];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeTokens([Rgb; 25]);

impl ThemeTokens {
    pub fn default_for(variant: ThemeVariant) -> Self {
        Self(match variant {
            ThemeVariant::Light => LIGHT_TOKENS,
            ThemeVariant::Dark => DARK_TOKENS,
        })
    }

    pub fn set(&mut self, token: ThemeToken, rgb: Rgb) {
        self.0[token as usize] = rgb;
    }
}

impl Index<ThemeToken> for ThemeTokens {
    type Output = Rgb;

    fn index(&self, token: ThemeToken) -> &Rgb {
        &self.0[token as usize]
    }
}

macro_rules! commands {
    ($($command:ident $id:literal $label:literal $hotkey:literal;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Command {
            $($command,)*
        }

        impl Command {
            pub const ALL: [Command; 23] = [$(Command::$command,)*];

            pub fn id(self) -> &'static str {
                match self {
                    $(Command::$command => $id,)*
                }
            }

            pub fn label(self) -> &'static str {
                match self {
                    $(Command::$command => $label,)*
                }
            }

            pub fn default_hotkey(self) -> Hotkey {
                match self {
                    $(Command::$command => Hotkey($hotkey.into()),)*
                }
            }
        }
    };
}

commands! {
    ToggleTopBar "toggleTopBar" "Toggle Top Bar" "Mod+Shift+T";
    LoadScript "loadScript" "Load Script" "Mod+O";
    ToggleScript "toggleScript" "Toggle Script" "Mod+Shift+X";
    ToggleScriptsDialog "toggleScriptsDialog" "Toggle Scripts Dialog" "Mod+Shift+S";
    ToggleOptionsMenu "toggleOptionsMenu" "Toggle Options Menu" "Mod+Shift+,";
    OpenEnvironment "openEnvironment" "Open Environment" "Alt+E";
    OpenLoaderGrabber "openLoaderGrabber" "Open Loader/Grabber" "";
    OpenFollower "openFollower" "Open Follower" "Alt+F";
    OpenPackets "openPackets" "Open Packets" "";
    ToggleAutoattack "toggleAutoattack" "Toggle Auto Attack" "Alt+A";
    ToggleFollower "toggleFollower" "Toggle Follower" "Alt+Shift+F";
    ToggleBank "toggleBank" "Toggle Bank" "Mod+B";
    ToggleInfiniteRange "toggleInfiniteRange" "Toggle Infinite Range" "Alt+I";
    ToggleProvokeCell "toggleProvokeCell" "Toggle Provoke Cell" "";
    ToggleEnemyMagnet "toggleEnemyMagnet" "Toggle Enemy Magnet" "";
    ToggleInterfaceOnlyRendering "toggleInterfaceOnlyRendering" "Toggle Interface-Only Rendering" "Alt+L";
    ToggleMinimalRendering "toggleMinimalRendering" "Toggle Minimal Rendering" "Alt+Shift+L";
    ToggleHidePlayers "toggleHidePlayers" "Toggle Hide Players" "";
    ToggleSkipCutscenes "toggleSkipCutscenes" "Toggle Skip Cutscenes" "";
    ToggleAntiCounter "toggleAntiCounter" "Toggle Anti-Counter" "";
    ToggleAnimations "toggleAnimations" "Toggle Animations" "";
    ToggleCollisions "toggleCollisions" "Toggle Collisions" "";
    ToggleDeathAds "toggleDeathAds" "Toggle Death Ads" "";
}

impl Command {
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|command| command.id() == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hotkey(String);

const MODIFIERS: [&str; 5] = ["Mod", "Control", "Alt", "Shift", "Meta"];

impl Hotkey {
    pub fn parse(text: &str) -> Option<Self> {
        let text = trim(text);
        if text.is_empty() {
            return Some(Self(String::new()));
        }
        let plus = text.contains('+');
        let separator = |c: char| if plus { c == '+' } else { is_space(c) };
        let parts: Vec<String> = text
            .split(separator)
            .map(trim)
            .filter(|part| !part.is_empty())
            .map(key)
            .collect();
        let distinct: HashSet<String> = parts.iter().map(|part| part.to_lowercase()).collect();
        if parts.is_empty() || distinct.len() != parts.len() {
            return None;
        }
        let mut keys = parts
            .iter()
            .filter(|part| !MODIFIERS.contains(&part.as_str()));
        let (Some(key), None) = (keys.next(), keys.next()) else {
            return None;
        };
        let modifiers = MODIFIERS
            .into_iter()
            .filter(|modifier| parts.iter().any(|part| part == modifier));
        Some(Self(
            modifiers
                .chain([key.as_str()])
                .collect::<Vec<_>>()
                .join("+"),
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_unbound(&self) -> bool {
        self.0.is_empty()
    }
}

fn key(part: &str) -> String {
    let alias = match part.to_lowercase().as_str() {
        "alt" | "option" => Some("Alt"),
        "control" | "ctrl" => Some("Control"),
        "command" | "cmd" | "meta" => Some("Meta"),
        "mod" => Some("Mod"),
        "shift" => Some("Shift"),
        _ => None,
    };
    match alias {
        Some(alias) => alias.into(),
        None if part.len() == 1 && part.as_bytes()[0].is_ascii_alphabetic() => {
            part.to_ascii_uppercase()
        }
        None => part.into(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hotkeys([Hotkey; 23]);

impl Default for Hotkeys {
    fn default() -> Self {
        Self(Command::ALL.map(Command::default_hotkey))
    }
}

impl Hotkeys {
    pub fn get(&self, command: Command) -> &Hotkey {
        &self.0[command as usize]
    }

    pub(crate) fn set(&mut self, command: Command, hotkey: Hotkey) {
        self.0[command as usize] = hotkey;
    }

    fn first_duplicate(&self) -> Option<Command> {
        let mut seen = Vec::new();
        Command::ALL.into_iter().find(|&command| {
            let hotkey = self.get(command);
            if hotkey.is_unbound() {
                return false;
            }
            let key = hotkey.as_str().to_lowercase();
            let taken = seen.contains(&key);
            seen.push(key);
            taken
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreferencesPatch {
    pub check_for_updates: Option<bool>,
    pub launch_mode: Option<LaunchMode>,
    pub show_game_username_in_window_title: Option<bool>,
    pub use_game_tabs: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppearancePatch {
    pub theme_mode: Option<ThemeMode>,
    pub reduce_motion: Option<MotionMode>,
    pub use_cursor_pointers: Option<bool>,
    pub light: ThemePatch,
    pub dark: ThemePatch,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemePatch {
    pub tokens: Vec<(ThemeToken, Option<Rgb>)>,
    pub sans_font: Option<Font>,
    pub mono_font: Option<Font>,
    pub sans_font_size: Option<FontSize>,
    pub mono_font_size: Option<FontSize>,
    pub rounding: Option<Rounding>,
}

pub(crate) enum Update {
    Preferences(PreferencesPatch),
    Appearance(AppearancePatch),
    Hotkeys(Vec<(Command, Option<Hotkey>)>),
    ResetAppearance,
    ResetHotkeys,
}

impl AppSettings {
    pub(crate) fn apply(&self, update: Update) -> Result<AppSettings, SettingsError> {
        let mut next = self.clone();
        match update {
            Update::Preferences(patch) => {
                let preferences = &mut next.preferences;
                assign(&mut preferences.check_for_updates, patch.check_for_updates);
                assign(&mut preferences.launch_mode, patch.launch_mode);
                assign(
                    &mut preferences.show_game_username_in_window_title,
                    patch.show_game_username_in_window_title,
                );
                assign(&mut preferences.use_game_tabs, patch.use_game_tabs);
            }
            Update::Appearance(patch) => {
                let appearance = &mut next.appearance;
                assign(&mut appearance.theme_mode, patch.theme_mode);
                assign(&mut appearance.reduce_motion, patch.reduce_motion);
                assign(
                    &mut appearance.use_cursor_pointers,
                    patch.use_cursor_pointers,
                );
                let themes = &mut appearance.themes;
                patch_theme(&mut themes.light, ThemeVariant::Light, patch.light);
                patch_theme(&mut themes.dark, ThemeVariant::Dark, patch.dark);
            }
            Update::Hotkeys(bindings) => {
                for (command, hotkey) in bindings {
                    let hotkey = hotkey.unwrap_or_else(|| command.default_hotkey());
                    next.hotkeys.set(command, hotkey);
                }
                if let Some(command) = next.hotkeys.first_duplicate() {
                    return Err(SettingsError::HotkeyTaken(command));
                }
            }
            Update::ResetAppearance => next.appearance = Appearance::default(),
            Update::ResetHotkeys => next.hotkeys = Hotkeys::default(),
        }
        Ok(next)
    }
}

fn assign<T>(field: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *field = value;
    }
}

fn patch_theme(profile: &mut ThemeProfile, variant: ThemeVariant, patch: ThemePatch) {
    let defaults = ThemeTokens::default_for(variant);
    for (token, rgb) in patch.tokens {
        profile.tokens.set(token, rgb.unwrap_or(defaults[token]));
    }
    assign(&mut profile.sans_font, patch.sans_font);
    assign(&mut profile.mono_font, patch.mono_font);
    assign(&mut profile.sans_font_size, patch.sans_font_size);
    assign(&mut profile.mono_font_size, patch.mono_font_size);
    assign(&mut profile.rounding, patch.rounding);
}
