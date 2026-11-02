pub mod auras;
pub mod auth;
mod baselines;
mod combat;
pub mod combat_state;
pub(crate) mod commands;
mod entity;
mod item;
mod item_mutations;
mod items;
pub mod monster_drops;
pub mod packet;
mod queries;
pub(crate) use queries::answer;
pub mod quests;
mod readiness;
pub mod shops;
pub(crate) mod wire;
pub mod world;

pub use item::decode_item;
pub use items::{ItemStore, LiveItem};
pub use readiness::{Avatar, PlayerReadiness, ReadinessKey, ReadinessReceipts, ReadinessSnapshot};

pub use auth::AuthState;
use lucent_protocol::{Packet, PacketDirection, event::ProjectionEvent, item::ItemContainer};

#[derive(Debug, Default)]
pub struct ProjectionOutput {
    pub events: Vec<ProjectionEvent>,
}

#[derive(Debug)]
pub enum PacketProjection {
    Applied(ProjectionOutput),
    Unhandled,
    NeedHydration(PendingHydration),
}

#[derive(Debug)]
pub struct PendingHydration {
    pub request: world::HydrationRequest,
    continuation: world::WorldContinuation,
    epoch: u64,
}

#[derive(Debug, thiserror::Error)]
#[error("hydration belongs to an earlier connection")]
pub struct StaleHydration;

#[derive(Debug, Default)]
pub struct Projection {
    pub auth: AuthState,
    pub items: ItemStore,
    pub receipts: ReadinessReceipts,
    pub readiness: PlayerReadiness,
    pub quests: quests::QuestStore,
    pub shops: shops::ShopStore,
    pub world: world::WorldStore,
    combat: combat_state::CounterWindows,
    session_logged_in: bool,
    pub(crate) connected: Option<bool>,
}

impl Projection {
    pub fn connection(&mut self, status: &str) -> ProjectionOutput {
        match status {
            "OnConnectionLost" | "OnConnectionFailed" => {
                self.auth.logged_in = false;
                self.connected = Some(false);
            }
            "OnConnection" => self.connected = Some(true),
            _ => return ProjectionOutput::default(),
        }
        self.readiness.logged_out = false;
        let logout = self.session_logged_in;
        self.session_logged_in = false;
        self.reset();
        ProjectionOutput {
            events: if logout {
                vec![ProjectionEvent::Logout]
            } else {
                vec![]
            },
        }
    }

    pub(crate) fn reset(&mut self) {
        self.combat = combat_state::CounterWindows::default();
        self.world.clear_area();
        self.world.clear_self();
        self.quests.clear();
        self.shops.clear();
        self.items.clear();
        self.receipts.reset();
    }

    pub(crate) fn login_if_ready(&mut self) -> Option<ProjectionEvent> {
        if self.session_logged_in || !self.current_readiness().missing.is_empty() {
            return None;
        }
        self.session_logged_in = true;
        Some(ProjectionEvent::Login)
    }

    pub fn readiness(&self, map_name: &str, self_bound: bool) -> ReadinessSnapshot {
        let mut snapshot = self.receipts.inspect(
            self.items.hydration(ItemContainer::House) > 0,
            self.items.hydration(ItemContainer::Inventory) > 0,
            map_name,
            self_bound,
        );
        snapshot.can_act = self.readiness.can_act();
        snapshot
    }

    pub fn current_readiness(&self) -> ReadinessSnapshot {
        self.readiness(&self.world.map.name, self.world.get_me().is_some())
    }

    pub(crate) fn refresh_readiness(&mut self, joined: bool) {
        let epoch = self.receipts.epoch();
        let connected = self.connected == Some(true) && !self.readiness.logged_out;
        let avatar = if joined || epoch != self.readiness.epoch || !connected {
            Avatar::Unknown
        } else {
            self.readiness.avatar
        };
        self.readiness = PlayerReadiness {
            epoch,
            connected,
            logged_out: self.readiness.logged_out,
            facts: self.current_readiness().missing.is_empty(),
            avatar,
        };
    }

    pub fn packet(
        &mut self,
        packet: &Packet,
        now_ms: f64,
        bridge_available: bool,
    ) -> PacketProjection {
        let result = self.project_packet(packet, now_ms, bridge_available);
        if let PacketProjection::Applied(output) = &result {
            self.combat.observe(&output.events, now_ms);
        }
        result
    }

    fn project_packet(
        &mut self,
        packet: &Packet,
        now_ms: f64,
        bridge_available: bool,
    ) -> PacketProjection {
        if baselines::project_baseline(&mut self.items, &mut self.receipts, packet) {
            return PacketProjection::Applied(ProjectionOutput::default());
        }
        let self_id = self.world.get_me().map(|player| player.data().entity_id);
        if let Some(output) = item_mutations::project_items(&mut self.items, packet, self_id) {
            return PacketProjection::Applied(output);
        }
        if let Some(output) = quests::project_quests(&mut self.quests, packet) {
            return PacketProjection::Applied(output);
        }
        if let Some(output) = shops::project_shop(&mut self.shops, &mut self.items, packet) {
            return PacketProjection::Applied(output);
        }
        if let Some(output) = combat::project_combat(&mut self.world, packet, now_ms) {
            return PacketProjection::Applied(output);
        }
        if packet.direction() == PacketDirection::Extension {
            if let Some(output) = auras::project_standalone_auras(&mut self.world, packet, now_ms) {
                return PacketProjection::Applied(output);
            }
            if packet.command() == "monsterDrops"
                && let Some(response) = monster_drops::decode_monster_drop_response(packet)
            {
                if let Some(id) = response.monster_map_id
                    && !self.world.set_monster_drops(id, response.drops)
                {
                    tracing::warn!(
                        command = "monsterDrops",
                        monster_map_id = id,
                        "drop response references an unknown monster"
                    );
                }
                return PacketProjection::Applied(response.output);
            }
        }
        world::project_world(
            &mut self.world,
            &mut self.items,
            &mut self.receipts,
            &self.auth.username,
            packet,
            now_ms,
            bridge_available,
        )
        .map_or(PacketProjection::Unhandled, |progress| {
            self.world_result(progress)
        })
    }

    pub fn resume_hydration(
        &mut self,
        pending: PendingHydration,
        result: world::HydrationResult,
        now_ms: f64,
    ) -> Result<PacketProjection, StaleHydration> {
        if pending.epoch != self.receipts.epoch() {
            return Err(StaleHydration);
        }
        let progress = world::resume_world(
            &mut self.world,
            &mut self.receipts,
            pending.continuation,
            result,
            now_ms,
        );
        let result = self.world_result(progress);
        if let PacketProjection::Applied(output) = &result {
            self.combat.observe(&output.events, now_ms);
        }
        Ok(result)
    }

    fn world_result(&self, progress: world::WorldProgress) -> PacketProjection {
        match progress {
            world::WorldProgress::Complete(output) => PacketProjection::Applied(output),
            world::WorldProgress::NeedHydration {
                request,
                continuation,
            } => PacketProjection::NeedHydration(PendingHydration {
                request,
                continuation,
                epoch: self.receipts.epoch(),
            }),
        }
    }
}
