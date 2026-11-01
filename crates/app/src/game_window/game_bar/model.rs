use lucent_protocol::{
    auto_relogin::{AutoReloginPhase, AutoReloginRecords},
    auto_zone::{AutoZoneMap, AutoZonePatch},
    command::Command,
    settings::{GameSettings, RenderingMode, SettingsPatch},
};
use std::path::PathBuf;

pub use lucent_protocol::script::ScriptPhase as Phase;

#[derive(Clone, Debug, PartialEq)]
pub enum BarAction {
    Load,
    Start,
    Stop,
    Kill,
    Toggle(Toggle, bool),
    WalkSpeed(f64),
    Fps(f64),
    Rendering(RenderingMode),
    CustomName(Option<String>),
    CustomGuild(Option<String>),
    ZoneEnabled(bool),
    ZoneMap(Option<AutoZoneMap>),
    ReloginEnabled(bool),
    Delay(f64),
    Server(Option<String>),
    Attack(bool),
    RefreshServers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    InfiniteRange,
    ProvokeCell,
    EnemyMagnet,
    HidePlayers,
    SkipCutscenes,
    AntiCounter,
    Animations,
    Collisions,
    DeathAds,
}

impl Toggle {
    pub const ALL: [Self; 9] = [
        Self::InfiniteRange,
        Self::ProvokeCell,
        Self::EnemyMagnet,
        Self::HidePlayers,
        Self::SkipCutscenes,
        Self::AntiCounter,
        Self::Animations,
        Self::Collisions,
        Self::DeathAds,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::InfiniteRange => "Infinite Range",
            Self::ProvokeCell => "Provoke Cell",
            Self::EnemyMagnet => "Enemy Magnet",
            Self::HidePlayers => "Hide Players",
            Self::SkipCutscenes => "Skip Cutscenes",
            Self::AntiCounter => "Anti-Counter",
            Self::Animations => "Animations",
            Self::Collisions => "Collisions",
            Self::DeathAds => "Death Ads",
        }
    }
    pub fn value(self, s: &GameSettings) -> bool {
        match self {
            Self::InfiniteRange => s.infinite_range_enabled,
            Self::ProvokeCell => s.provoke_cell_enabled,
            Self::EnemyMagnet => s.enemy_magnet_enabled,
            Self::HidePlayers => !s.other_players_visible,
            Self::SkipCutscenes => s.skip_cutscenes_enabled,
            Self::AntiCounter => s.anti_counter_enabled,
            Self::Animations => s.animations_enabled,
            Self::Collisions => s.collisions_enabled,
            Self::DeathAds => s.death_ads_visible,
        }
    }
}

#[derive(Default)]
pub struct BarState {
    pub loaded: Option<PathBuf>,
    pub loading: bool,
    pub error: Option<String>,
    pub phase: Phase,
    pub script: Option<String>,
    pub last_end: Option<String>,
    pub settings: GameSettings,
    pub zone_enabled: bool,
    pub zone_map: Option<AutoZoneMap>,
    pub relogin_enabled: bool,
    pub delay: f64,
    pub server: Option<String>,
    pub servers: Vec<String>,
    pub relogin_status: String,
    pub attack_enabled: bool,
    pub attack_profile: String,
    pub attack_issue: String,
}

impl BarState {
    pub fn phase_label(&self) -> &'static str {
        match self.phase {
            Phase::Idle => "Idle",
            Phase::Waiting => "Waiting for login",
            Phase::Starting => "Starting",
            Phase::Running => "Running",
            Phase::Stopping => "Stopping",
        }
    }
    pub fn script_action(&self) -> Option<BarAction> {
        if self.loading {
            return None;
        }
        match self.phase {
            Phase::Idle => self.loaded.as_ref().map(|_| BarAction::Start),
            Phase::Waiting | Phase::Starting | Phase::Running => Some(BarAction::Stop),
            Phase::Stopping => Some(BarAction::Kill),
        }
    }
    pub fn script_label(&self) -> &'static str {
        match self.phase {
            Phase::Idle => "Start",
            Phase::Stopping => "Kill",
            _ => "Stop",
        }
    }
}

pub const MAPS: [(AutoZoneMap, &str); 7] = [
    (AutoZoneMap::Ledgermayne, "Ledgermayne"),
    (AutoZoneMap::Moreskulls, "More Skulls"),
    (AutoZoneMap::Ultradage, "Ultra Dage"),
    (AutoZoneMap::Darkcarnax, "Dark Carnax"),
    (AutoZoneMap::Astralshrine, "Astral Shrine"),
    (AutoZoneMap::Queeniona, "Queen Iona"),
    (AutoZoneMap::Magnumopus, "Magnum Opus"),
];

pub fn command(action: BarAction) -> Option<Command> {
    let mut patch = SettingsPatch::default();
    match action {
        BarAction::Toggle(toggle, value) => match toggle {
            Toggle::InfiniteRange => patch.infinite_range_enabled = Some(value),
            Toggle::ProvokeCell => patch.provoke_cell_enabled = Some(value),
            Toggle::EnemyMagnet => patch.enemy_magnet_enabled = Some(value),
            Toggle::HidePlayers => patch.other_players_visible = Some(!value),
            Toggle::SkipCutscenes => patch.skip_cutscenes_enabled = Some(value),
            Toggle::AntiCounter => patch.anti_counter_enabled = Some(value),
            Toggle::Animations => patch.animations_enabled = Some(value),
            Toggle::Collisions => patch.collisions_enabled = Some(value),
            Toggle::DeathAds => patch.death_ads_visible = Some(value),
        },
        BarAction::WalkSpeed(value) => patch.walk_speed = Some(value.round().clamp(1., 99.)),
        BarAction::Fps(value) => patch.frame_rate = Some(value.round().clamp(1., 60.)),
        BarAction::Rendering(value) => patch.rendering_mode = Some(value),
        BarAction::CustomName(value) => patch.custom_name = Some(value),
        BarAction::CustomGuild(value) => patch.custom_guild = Some(value),
        BarAction::ZoneEnabled(enabled) => {
            return Some(Command::PatchAutoZone {
                patch: AutoZonePatch {
                    enabled: Some(enabled),
                    ..Default::default()
                },
            });
        }
        BarAction::ZoneMap(map) => {
            return Some(Command::PatchAutoZone {
                patch: AutoZonePatch {
                    map: Some(map),
                    ..Default::default()
                },
            });
        }
        BarAction::ReloginEnabled(enabled) => {
            return Some(Command::SetAutoReloginEnabled { enabled });
        }
        BarAction::Delay(seconds) => {
            return Some(Command::SetAutoReloginDelay {
                delay_ms: seconds.clamp(0., 300.) * 1000.,
            });
        }
        BarAction::Server(server) => return Some(Command::SetAutoReloginServer { server }),
        BarAction::RefreshServers => return Some(Command::RefreshServers {}),
        BarAction::Attack(true) => {
            return Some(Command::EnableAutoAttack {
                profile: serde_json::Value::Null,
                target_priority: vec![],
            });
        }
        BarAction::Attack(false) => return Some(Command::DisableAutoAttack {}),
        BarAction::Load | BarAction::Start | BarAction::Stop | BarAction::Kill => return None,
    }
    Some(Command::UpdateSettings { patch })
}

pub fn relogin_status(records: &AutoReloginRecords) -> String {
    if !records.config.enabled {
        return String::new();
    }
    let status = match records.status.phase {
        AutoReloginPhase::WaitingDelay | AutoReloginPhase::WaitingKick => "Waiting to reconnect",
        _ if !records.status.captured => "Waiting for login",
        AutoReloginPhase::AwaitingPlayerReady
        | AutoReloginPhase::AwaitingServerSelect
        | AutoReloginPhase::Backoff { .. }
        | AutoReloginPhase::LoggingIn
        | AutoReloginPhase::SelectingServer => "Reconnecting",
        AutoReloginPhase::Armed | AutoReloginPhase::AwaitingSession | AutoReloginPhase::Failed => {
            return records.status.last_error.clone().unwrap_or_default();
        }
    };
    match records.status.attempts_remaining {
        None => status.into(),
        Some(attempts) if attempts <= 0. => format!("{status} (final attempt)"),
        Some(1.) => format!("{status} (1 retry left)"),
        Some(attempts) => format!("{status} ({attempts} retries left)"),
    }
}
