use lucent_protocol::{
    aura::AuraData,
    auth::AuthSnapshot,
    auto_zone::AutoZoneConfig,
    event::{AuraKind, MapState},
    item::{ItemContainer, ItemSnapshot},
    monster_drop::MonsterDrop,
    query::{Answer, ItemSelector, MonsterSelector, PlayerSelector, Query},
    quest::QuestData,
    readiness::ReadinessSnapshot,
    settings::GameSettings,
    shop::ShopData,
    world::{MonsterSnapshot, PlayerSnapshot},
};

use super::{Projection, entity::LiveAura};

fn auras(auras: Vec<LiveAura>, kind: Option<AuraKind>) -> Vec<AuraData> {
    auras
        .iter()
        .map(LiveAura::data)
        .filter(|aura| kind.is_none_or(|kind| kind == aura.kind))
        .collect()
}

struct State<'a> {
    environment: &'a parking_lot::RwLock<lucent_protocol::environment::EnvironmentState>,
    projection: &'a Projection,
    settings: &'a GameSettings,
    auto_attack: &'a parking_lot::RwLock<lucent_protocol::auto_attack::AutoAttackRecords>,
    auto_relogin: &'a parking_lot::RwLock<lucent_protocol::auto_relogin::AutoReloginRecords>,
    auto_zone: &'a parking_lot::RwLock<AutoZoneConfig>,
}

pub fn answer(
    projection: &Projection,
    settings: &GameSettings,
    auto_zone: &parking_lot::RwLock<AutoZoneConfig>,
    auto_attack: &parking_lot::RwLock<lucent_protocol::auto_attack::AutoAttackRecords>,
    auto_relogin: &parking_lot::RwLock<lucent_protocol::auto_relogin::AutoReloginRecords>,
    environment: &parking_lot::RwLock<lucent_protocol::environment::EnvironmentState>,
    query: Query,
) -> String {
    lucent_protocol::query::answer(
        &State {
            environment,
            projection,
            settings,
            auto_zone,
            auto_attack,
            auto_relogin,
        },
        query,
    )
}

impl Answer for State<'_> {
    fn environment(&self) -> lucent_protocol::environment::EnvironmentState {
        self.environment.read().clone()
    }
    fn combat_state(
        &self,
        priorities: Vec<MonsterSelector>,
    ) -> Vec<lucent_protocol::combat_state::CombatMonsterState> {
        self.projection.combat_monsters(
            super::combat_state::now_ms(),
            &priorities,
            self.settings.anti_counter_enabled,
        )
    }
    fn auto_attack(&self) -> lucent_protocol::auto_attack::AutoAttackRecords {
        self.auto_attack.read().clone()
    }
    fn auto_relogin(&self) -> lucent_protocol::auto_relogin::AutoReloginRecords {
        self.auto_relogin.read().clone()
    }
    fn auto_zone(&self) -> AutoZoneConfig {
        self.auto_zone.read().clone()
    }

    fn settings(&self) -> GameSettings {
        self.settings.clone()
    }

    fn map(&self) -> MapState {
        self.projection.world.map.clone()
    }

    fn me(&self) -> Option<PlayerSnapshot> {
        self.projection
            .world
            .get_me()
            .map(|player| player.snapshot_data())
    }

    fn self_entity_id(&self) -> Option<i64> {
        self.projection.world.self_entity_id
    }

    fn player(&self, selector: PlayerSelector) -> Option<PlayerSnapshot> {
        self.projection
            .world
            .resolve_player(&selector)
            .map(|player| player.snapshot_data())
    }

    fn players(&self) -> Vec<PlayerSnapshot> {
        self.projection
            .world
            .players()
            .iter()
            .map(|player| player.snapshot_data())
            .collect()
    }

    fn player_auras(&self, entity_id: i64, aura_kind: Option<AuraKind>) -> Vec<AuraData> {
        self.projection
            .world
            .get_player_by_id(entity_id)
            .map_or_else(Vec::new, |player| auras(player.auras(), aura_kind))
    }

    fn monster(&self, selector: MonsterSelector) -> Option<MonsterSnapshot> {
        self.projection
            .world
            .resolve_monster(&selector)
            .map(|monster| monster.snapshot_data())
    }

    fn monsters(&self) -> Vec<MonsterSnapshot> {
        self.projection
            .world
            .monsters()
            .iter()
            .map(|monster| monster.snapshot_data())
            .collect()
    }

    fn monster_auras(&self, monster_map_id: i64, aura_kind: Option<AuraKind>) -> Vec<AuraData> {
        self.projection
            .world
            .get_monster(monster_map_id)
            .map_or_else(Vec::new, |monster| auras(monster.auras(), aura_kind))
    }

    fn monster_drops(&self, monster_map_id: i64) -> Option<Vec<MonsterDrop>> {
        self.projection
            .world
            .monster_drops(monster_map_id)
            .map(<[MonsterDrop]>::to_vec)
    }

    fn cells(&self) -> Vec<String> {
        self.projection.world.cells()
    }

    fn cell_pads(&self) -> Vec<String> {
        self.projection.world.cell_pads()
    }

    fn item(&self, container: ItemContainer, selector: ItemSelector) -> Option<ItemSnapshot> {
        self.projection
            .items
            .resolve(container, &selector)
            .map(|item| item.data().snapshot_data())
    }

    fn items(&self, container: ItemContainer) -> Vec<ItemSnapshot> {
        self.projection
            .items
            .all(container)
            .iter()
            .map(|item| item.data().snapshot_data())
            .collect()
    }

    fn item_hydration(&self, container: ItemContainer) -> u64 {
        self.projection.items.hydration(container)
    }

    fn readiness(&self) -> ReadinessSnapshot {
        self.projection.current_readiness()
    }

    fn quest(&self, id: f64) -> Option<QuestData> {
        self.projection
            .quests
            .get(id)
            .map(|quest| quest.data().clone())
    }

    fn quests(&self) -> Vec<QuestData> {
        self.projection
            .quests
            .all()
            .iter()
            .map(|quest| quest.data().clone())
            .collect()
    }

    fn accepted_quests(&self) -> Vec<QuestData> {
        self.projection
            .quests
            .accepted()
            .iter()
            .map(|quest| quest.data().clone())
            .collect()
    }

    fn shop(&self) -> Option<ShopData> {
        self.projection.shops.get().map(|shop| shop.data())
    }

    fn auth(&self) -> AuthSnapshot {
        self.projection.auth.snapshot()
    }
}
