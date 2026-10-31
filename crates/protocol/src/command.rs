use crate::auth::{CredentialsSnapshot, ServerSnapshot};
use crate::auto_zone::{AutoZoneConfig, AutoZonePatch};
use crate::query::{ItemSelector, MonsterSelector};
use crate::quest::QuestData;
use crate::settings::{GameSettings, SettingsPatch};
use crate::world::MonsterSnapshot;

#[derive(Clone, Copy, Debug, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum RefreshContainer {
    Bank,
    House,
    Inventory,
}

macro_rules! commands {
    ($($variant:ident { $($field:ident: $input:ty),* } => $output:ty),* $(,)?) => {
        #[derive(Debug, serde::Deserialize, ts_rs::TS)]
        #[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
        #[ts(optional_fields = nullable)]
        pub enum Command {
            $($variant { $($field: $input),* }),*
        }

        pub fn command_outputs(config: &ts_rs::Config) -> String {
            let mut declaration = String::from("export type CommandOutputs = { ");
            $(
                let mut key = stringify!($variant).to_owned();
                key[..1].make_ascii_lowercase();
                declaration.push_str(&format!("{}: {}, ", key, <$output as ts_rs::TS>::name(config)));
            )*
            declaration.push_str("};\n");
            declaration
        }
    };
}

commands! {
    UpdateEnvironment { edit: crate::environment::EnvironmentEdit } => crate::environment::EnvironmentState,
    RefreshCombatState {} => crate::combat_state::CombatSnapshot,
    EnableAutoAttack { profile: serde_json::Value, target_priority: Vec<String> } => crate::auto_attack::AutoAttackRecords,
    DisableAutoAttack {} => crate::auto_attack::AutoAttackRecords,
    SetAutoReloginEnabled { enabled: bool } => crate::auto_relogin::AutoReloginRecords,
    SetAutoReloginDelay { delay_ms: f64 } => crate::auto_relogin::AutoReloginRecords,
    SetAutoReloginServer { server: Option<String> } => crate::auto_relogin::AutoReloginRecords,
    PatchAutoZone { patch: AutoZonePatch } => AutoZoneConfig,
    UpdateSettings { patch: SettingsPatch } => GameSettings,
    ReapplySettings { actions_only: bool } => (),
    RefreshContainer { container: RefreshContainer } => usize,
    ConfirmShopClosed { shop_id: Option<i64> } => bool,
    RejectDrop { selector: ItemSelector } => bool,
    RefreshCells {} => Vec<String>,
    RefreshCellPads {} => Vec<String>,
    HydrateMonster { selector: MonsterSelector } => Option<MonsterSnapshot>,
    Login { username: String, password: String } => bool,
    Logout {} => (),
    RefreshCredentials {} => CredentialsSnapshot,
    RefreshServers {} => Vec<ServerSnapshot>,
    RefreshLoggedIn {} => bool,
    RefreshAcceptedQuests {} => Vec<QuestData>,
    RefreshEquippedConsumable {} => Option<i64>,
}
