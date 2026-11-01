use crate::aura::AuraData;
use crate::auth::AuthSnapshot;
use crate::auto_zone::AutoZoneConfig;
use crate::event::{AuraKind, MapState};
use crate::item::{ItemContainer, ItemSnapshot};
use crate::monster_drop::MonsterDrop;
use crate::quest::QuestData;
use crate::readiness::ReadinessSnapshot;
use crate::settings::GameSettings;
use crate::shop::ShopData;
use crate::world::{MonsterSnapshot, PlayerSnapshot};

#[derive(Clone, Debug, serde::Deserialize, ts_rs::TS)]
#[serde(untagged)]
pub enum PlayerSelector {
    EntityId(f64),
    Username(String),
}

#[derive(Clone, Debug, serde::Deserialize, ts_rs::TS)]
#[serde(untagged, rename_all_fields = "camelCase")]
pub enum MonsterSelector {
    Id(f64),
    Name(String),
    MonsterMapId { monster_map_id: f64 },
    Named { name: String },
}

#[derive(Clone, Debug, serde::Deserialize, ts_rs::TS)]
#[serde(untagged, rename_all_fields = "camelCase")]
pub enum ItemSelector {
    Id(f64),
    Name(String),
    ShopItemId { shop_item_id: f64 },
    ItemId { item_id: f64 },
    Named { name: String },
}

macro_rules! queries {
    ($($variant:ident { $($field:ident: $input:ty),* } => $output:ty),* $(,)?) => {
        #[derive(Debug, serde::Deserialize, ts_rs::TS)]
        #[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
        #[ts(optional_fields = nullable)]
        pub enum Query {
            $($variant { $($field: $input),* }),*
        }

        paste::paste! {
            pub trait Answer {
                $(fn [<$variant:snake>](&self, $($field: $input),*) -> $output;)*
            }

            pub fn answer(state: &impl Answer, query: Query) -> String {
                match query {
                    $(Query::$variant { $($field),* } =>
                        serde_json::to_string(&state.[<$variant:snake>]($($field),*)).unwrap()),*
                }
            }
        }

        pub fn query_outputs(config: &ts_rs::Config) -> String {
            let mut declaration = String::from("export type QueryOutputs = { ");
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

queries! {
    Environment {} => crate::environment::EnvironmentState,
    CombatState { priorities: Vec<MonsterSelector> } => Vec<crate::combat_state::CombatMonsterState>,
    AutoAttack {} => crate::auto_attack::AutoAttackRecords,
    AutoRelogin {} => crate::auto_relogin::AutoReloginRecords,
    AutoZone {} => AutoZoneConfig,
    Settings {} => GameSettings,
    Map {} => MapState,
    Me {} => Option<PlayerSnapshot>,
    SelfEntityId {} => Option<i64>,
    Player { selector: PlayerSelector } => Option<PlayerSnapshot>,
    Players {} => Vec<PlayerSnapshot>,
    PlayerAuras { entity_id: i64, aura_kind: Option<AuraKind> } => Vec<AuraData>,
    Monster { selector: MonsterSelector } => Option<MonsterSnapshot>,
    Monsters {} => Vec<MonsterSnapshot>,
    MonsterAuras { monster_map_id: i64, aura_kind: Option<AuraKind> } => Vec<AuraData>,
    MonsterDrops { monster_map_id: i64 } => Option<Vec<MonsterDrop>>,
    Cells {} => Vec<String>,
    CellPads {} => Vec<String>,
    Item { container: ItemContainer, selector: ItemSelector } => Option<ItemSnapshot>,
    Items { container: ItemContainer } => Vec<ItemSnapshot>,
    ItemHydration { container: ItemContainer } => u64,
    Readiness {} => ReadinessSnapshot,
    Quest { id: f64 } => Option<QuestData>,
    Quests {} => Vec<QuestData>,
    AcceptedQuests {} => Vec<QuestData>,
    Shop {} => Option<ShopData>,
    Auth {} => AuthSnapshot,
}
