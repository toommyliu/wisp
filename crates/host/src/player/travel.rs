use super::Running;
use crate::{
    bridge::{Error, Value},
    game::{jobs::JobGate, travel::*},
};
use lucent_protocol::{command::Command, world::EntityState};
use std::time::Instant;

impl Running {
    pub(super) fn safe_move_call(
        &mut self,
        guard: &crate::game::jobs::safe_move::Guard,
        deadline: Instant,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, Error> {
        if !guard.allows(
            self.operation_packets.binding(),
            &self.projection.read().auth.username,
        ) {
            return Err(Error::Stopped);
        }
        self.travel_call(&guard.gate, deadline, method, args)
    }
    pub(super) fn observe_safe_move(
        &mut self,
        reads: Reads,
        own_house: crate::game::safe_move::HouseRead,
        guard: &crate::game::jobs::safe_move::Guard,
        deadline: Instant,
    ) -> Result<crate::game::safe_move::Snapshot, Error> {
        if !guard.allows(
            self.operation_packets.binding(),
            &self.projection.read().auth.username,
        ) {
            return Err(Error::Stopped);
        }
        let travel = self.observe_travel(reads, &guard.gate, deadline);
        let equipped_house = self
            .projection
            .read()
            .items
            .all(lucent_protocol::item::ItemContainer::House)
            .iter()
            .any(|item| {
                let item = item.data();
                item.category == "House" && item.equipped
            });
        let read_house = match own_house {
            crate::game::safe_move::HouseRead::None => false,
            crate::game::safe_move::HouseRead::IfEquipped => equipped_house,
            crate::game::safe_move::HouseRead::Always => true,
        };
        let own_house = if read_house {
            let value = self.safe_move_call(
                guard,
                deadline,
                "flash.callGameFunction0",
                vec![Value::String("world.isMyHouse".into())],
            )?;
            matches!(value, Value::String(json) if serde_json::from_str::<bool>(&json).ok() == Some(true))
        } else {
            false
        };
        let state = self.projection.read();
        Ok(crate::game::safe_move::Snapshot {
            logged_in: state.auth.logged_in,
            username: state.auth.username.clone(),
            equipped_house,
            own_house,
            travel,
        })
    }

    pub(super) fn travel_call(
        &mut self,
        gate: &JobGate,
        deadline: Instant,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Value, Error> {
        if !gate.open() {
            return Err(Error::Stopped);
        }
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        self.invoke_command(name, args)
    }
    pub(super) fn observe_travel(
        &mut self,
        reads: Reads,
        gate: &JobGate,
        deadline: Instant,
    ) -> Snapshot {
        let mut snapshot = Snapshot::default();
        if reads.contains(Reads::LOAD) {
            let map = self.projection.read().world.map.clone();
            snapshot.map = map.name;
            snapshot.room = map.room_number.max(0.0) as u64;
            snapshot.loaded =
                self.travel_call(gate, deadline, "world.isLoaded", vec![]) == Ok(Value::Bool(true));
        }
        if reads.contains(Reads::READY)
            && self
                .projection
                .read()
                .current_readiness()
                .missing
                .is_empty()
        {
            let logged_in = if gate.open() && Instant::now() < deadline {
                self.command(Command::RefreshLoggedIn {})
                    .ok()
                    .and_then(|json| serde_json::from_str::<bool>(&json).ok())
                    .unwrap_or_else(|| self.projection.read().auth.logged_in)
            } else {
                false
            };
            let loaded =
                self.travel_call(gate, deadline, "world.isLoaded", vec![]) == Ok(Value::Bool(true));
            let player_loaded = self.travel_call(gate, deadline, "player.isLoaded", vec![])
                == Ok(Value::Bool(true));
            snapshot.ready = logged_in && loaded && player_loaded;
        }
        if reads.contains(Reads::TFER) {
            snapshot.tfer = self.travel_call(
                gate,
                deadline,
                "world.isActionAvailable",
                vec![Value::String("tfer".into())],
            ) == Ok(Value::Bool(true));
        }
        if reads.contains(Reads::STATE)
            || reads.contains(Reads::ALIVE)
            || reads.contains(Reads::EXIT)
        {
            let me = self.projection.read().world.get_me().map(|me| me.data());
            if let Some(me) = me {
                snapshot.alive = me.entity.alive();
                snapshot.state = match me.entity.state {
                    EntityState::Idle => CombatState::Idle,
                    EntityState::InCombat => CombatState::Combat,
                    EntityState::Dead => CombatState::Dead,
                };
                snapshot.cell = me.entity.cell;
                snapshot.pad = me.pad;
            } else if reads.contains(Reads::ALIVE) {
                let hp = self.travel_call(gate, deadline, "player.getHp", vec![]);
                let state = self.travel_call(gate, deadline, "player.getState", vec![]);
                snapshot.alive = matches!(hp, Ok(Value::Number(hp)) if hp > 0.0)
                    && matches!(state, Ok(Value::Number(state)) if state != 0.0);
            }
        }
        if reads.contains(Reads::LIVE) {
            snapshot.live_cell = self.travel_text(gate, deadline, "player.getCell");
            snapshot.live_pad = self.travel_text(gate, deadline, "player.getPad");
        }
        if reads.contains(Reads::EXIT) {
            if snapshot.cell.is_empty() || snapshot.pad.is_empty() {
                snapshot.loaded = self.travel_call(gate, deadline, "world.isLoaded", vec![])
                    == Ok(Value::Bool(true));
                if snapshot.loaded {
                    if snapshot.cell.is_empty() {
                        snapshot.live_cell = self.travel_text(gate, deadline, "player.getCell");
                    }
                    if snapshot.pad.is_empty() {
                        snapshot.live_pad = self.travel_text(gate, deadline, "player.getPad");
                    }
                }
            }
            snapshot.cells = if gate.open() && Instant::now() < deadline {
                self.refresh_cells(|player, name| {
                    lucent_protocol::bridge::to_json(player.travel_call(
                        gate,
                        deadline,
                        name,
                        vec![],
                    )?)
                    .ok_or_else(|| {
                        crate::projection::commands::decode_error(
                            name,
                            "the callback returned undefined",
                        )
                    })
                })
                .ok()
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_else(|| self.projection.read().world.cells())
            } else {
                vec![]
            };
            snapshot.monsters = self
                .projection
                .read()
                .world
                .monsters()
                .into_iter()
                .map(|monster| {
                    let data = monster.data();
                    Monster {
                        cell: data.entity.cell,
                        aggressive: data.aggressive,
                    }
                })
                .collect();
        }
        snapshot
    }
    fn travel_text(&mut self, gate: &JobGate, deadline: Instant, name: &str) -> String {
        match self.travel_call(gate, deadline, name, vec![]) {
            Ok(Value::String(value)) => value,
            _ => String::new(),
        }
    }
}
