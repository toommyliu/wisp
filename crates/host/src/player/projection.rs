use std::time::{SystemTime, UNIX_EPOCH};

use lucent_protocol::{
    PacketDirection,
    bridge::{from_json, to_json},
    event::ProjectionEvent,
    hydration::HydrationResult,
};

use super::Running;
use crate::bridge::{self, Error, Event, ScriptEvent, Value};
use crate::projection::{
    PacketProjection, Projection,
    packet::{PacketDecode, parse_packet},
};

impl Running {
    pub(super) fn drain_events(&mut self) {
        loop {
            let received = match self.loader_events.try_recv() {
                Ok(Some(event)) => Some(event),
                Err(Error::Overflow) => None,
                _ => break,
            };
            let mut projection = self.projection.write();
            let epoch = projection.receipts.epoch();
            let event = match received {
                None => {
                    self.invalidate_projection(&mut projection);
                    None
                }
                Some(event) => {
                    let projected = self.project_event(&mut projection, &event);
                    Some(ScriptEvent::Loader { event, projected })
                }
            };
            let invalidated = projection.receipts.epoch() != epoch;
            let ready = projection.current_readiness().missing.is_empty();
            if invalidated || event.as_ref().is_some_and(|event| matches!(event, ScriptEvent::Loader { projected, .. } if projected.iter().any(|event| matches!(event, ProjectionEvent::JoinMap { .. })))) { self.profile_area += 1; }
            if let Some(ScriptEvent::Loader { projected, .. }) = &event {
                for event in projected {
                    if let ProjectionEvent::MonsterDeath { monster_map_id } = event
                        && let Some(monster) = projection.world.get_monster(*monster_map_id)
                    {
                        self.publish(ScriptEvent::ProfileDeath {
                            area: self.profile_area,
                            key: crate::game::profile::TargetKey {
                                monster_map_id: *monster_map_id,
                                generation: monster.generation(),
                            },
                        });
                    }
                }
            }
            drop(projection);
            let countered: Vec<i64> = match &event {
                Some(ScriptEvent::Loader { projected, .. })
                    if self.settings.read().anti_counter_enabled =>
                {
                    projected
                        .iter()
                        .filter_map(|event| match event {
                            ProjectionEvent::CounterAttackStart { monster_map_id, .. } => {
                                Some(*monster_map_id)
                            }
                            _ => None,
                        })
                        .collect()
                }
                _ => Vec::new(),
            };
            if let Some(event) = event {
                self.publish(event);
            }
            for monster_map_id in countered {
                self.stop_attacking_countered(monster_map_id);
            }
            if ready && !self.settings_applied {
                self.settings_applied = true;
                let _ = self.reapply_settings(false);
            }
            if invalidated {
                self.publish(ScriptEvent::Invalidated);
            }
        }
    }

    fn stop_attacking_countered(&mut self, monster_map_id: i64) {
        let mut player = self.player.lock().unwrap();
        let targeted = bridge::invoke(&mut player, "combat.getTarget", vec![])
            .ok()
            .and_then(to_json)
            .is_some_and(|target| {
                target.get("type").and_then(serde_json::Value::as_str) == Some("monster")
                    && target
                        .get("monsterMapId")
                        .and_then(crate::projection::wire::wire_i64)
                        == Some(monster_map_id)
            });
        if targeted {
            let _ = bridge::invoke(&mut player, "combat.cancelAutoAttack", vec![]);
            let _ = bridge::invoke(&mut player, "combat.cancelTarget", vec![]);
        }
    }

    pub(super) fn publish(&self, event: ScriptEvent) {
        if let Some(((_, last), rest)) = self.script_events.split_last() {
            for (_, stream) in rest {
                if stream.accepts(&event) {
                    stream.send(event.clone());
                }
            }
            last.send(event);
        }
    }

    fn invalidate_projection(&self, projection: &mut Projection) {
        projection.reset();
        tracing::debug!(
            epoch = projection.receipts.epoch(),
            "projection invalidated after event overflow"
        );
    }

    fn project_event(&self, projection: &mut Projection, event: &Event) -> Vec<ProjectionEvent> {
        let Some(Value::String(raw)) = event.args.first() else {
            return vec![];
        };
        let direction = match event.name.as_str() {
            "onConnection" => {
                if matches!(
                    raw.as_str(),
                    "OnConnection" | "OnConnectionLost" | "OnConnectionFailed"
                ) {
                    self.operation_packets.invalidate();
                }
                let output = projection.connection(raw);
                tracing::debug!(
                    status = raw,
                    epoch = projection.receipts.epoch(),
                    "projection connection changed"
                );
                return output.events;
            }
            "onExtensionResponse" => PacketDirection::Extension,
            "packetFromClient" => PacketDirection::Client,
            "packetFromServer" => PacketDirection::Server,
            _ => return vec![],
        };
        let PacketDecode::Parsed(packet) = parse_packet(direction, raw) else {
            return vec![];
        };
        let mut progress = projection.packet(&packet, now_ms(), true);
        let mut projected = vec![];
        loop {
            match progress {
                PacketProjection::Applied(output) => {
                    projected = output.events;
                    break;
                }
                PacketProjection::Unhandled => break,
                PacketProjection::NeedHydration(pending) => {
                    tracing::debug!(
                        method = pending.request.method.name(),
                        "projection hydration requested"
                    );
                    let result = bridge::invoke(
                        &mut self.player.lock().unwrap(),
                        pending.request.method.name(),
                        pending
                            .request
                            .args
                            .iter()
                            .cloned()
                            .map(from_json)
                            .collect(),
                    )
                    .ok()
                    .and_then(to_json)
                    .map_or(HydrationResult::Unavailable, HydrationResult::Value);
                    let Ok(next) = projection.resume_hydration(pending, result, now_ms()) else {
                        break;
                    };
                    progress = next;
                }
            }
        }
        projected.extend(projection.login_if_ready());
        self.operation_packets.packet(&packet);
        tracing::debug!(
            command = packet.command(),
            direction = ?packet.direction(),
            epoch = projection.receipts.epoch(),
            missing = ?projection.current_readiness().missing,
            map = projection.world.map.name,
            cell = projection.world.get_me().map(|me| me.data().entity.cell.clone()),
            hp = projection.world.get_me().map(|me| me.data().entity.hp),
            players = projection.world.players().len(),
            monsters = projection.world.monsters().len(),
            inventory = projection.items.all(lucent_protocol::item::ItemContainer::Inventory).len(),
            "projection processed packet"
        );
        projected
    }
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{Bridge, Error as BridgeError, ScriptEvents, script_events};
    use crate::player::{Msg, Read, Surface, Tasks, Viewport, first_viewport, surface};
    use crate::projection::Projection;
    use lucent_protocol::{item::ItemContainer, readiness::ReadinessKey};
    use ruffle_core::{
        PlayerBuilder,
        external::{ExternalInterfaceProvider, Value as ExternalValue},
    };
    use serde_json::json;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    fn running(input: usize, output: usize) -> (Running, Bridge, ScriptEvents) {
        let (bridge, loader_events) = Bridge::with_capacity(input);
        let (producer, events) = script_events(output, crate::bridge::EventInterests::ALL);
        let now = Instant::now();
        (
            Running {
                settings: Default::default(),
                auto_zone: Default::default(),
                changes: Default::default(),
                settings_applied: false,
                rejected_settings: Default::default(),
                operation_packets: Default::default(),
                loader_events,
                script_events: vec![(crate::sessions::AttachmentId(0), producer)],
                projection: std::sync::Arc::new(parking_lot::RwLock::new(Projection::default())),
                player: PlayerBuilder::new().build(),
                tasks: Tasks::default(),
                viewport: Viewport::new(1024.0, 768.0, 1.0).unwrap(),
                profile_area: 0,
                #[cfg(feature = "testing")]
                travel_calls: None,
                last_tick: now,
                effects: vec![],
                drawn: 0,
                stats_since: now,
                surface: Surface::textures(surface::noop_gpu(), || {}),
            },
            bridge,
            events,
        )
    }

    fn queue(running: &mut Running, bridge: &Bridge, name: &str, raw: String) {
        running
            .player
            .lock()
            .unwrap()
            .mutate_with_update_context(|context| {
                bridge.call_method(context, name, &[ExternalValue::String(raw)]);
            });
    }

    fn extension(running: &mut Running, bridge: &Bridge, data: serde_json::Value) {
        queue(
            running,
            bridge,
            "onExtensionResponse",
            json!({"type":"json", "dataObj":data}).to_string(),
        );
    }

    fn area(running: &mut Running, bridge: &Bridge) {
        extension(
            running,
            bridge,
            json!({
                "cmd":"moveToArea", "areaName":"testroom-1",
                "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100}]
            }),
        );
    }

    fn read<R: serde::Serialize + Send + 'static>(
        running: &mut Running,
        f: impl FnOnce(&Projection) -> R + Send + 'static,
    ) -> R {
        let (request, reply) = Read::new(f);
        let _ = running.handle(Msg::Read(request));
        reply.recv_timeout(Duration::ZERO).unwrap()
    }

    fn quantity(state: &Projection, id: i64) -> Option<f64> {
        state
            .items
            .get(ItemContainer::Inventory, id)
            .map(|item| item.data().quantity)
    }

    fn accepting_operation(
        running: &mut Running,
    ) -> (
        crate::sessions::operations::Operations,
        mpsc::Receiver<Msg>,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        let (sender, inbox) = mpsc::channel();
        let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker = crate::sessions::operations::Operations::start(0).unwrap();
        worker
            .sender
            .send(Some(crate::sessions::operations::Request {
                binding: running.operation_packets.binding(),
                id: 42,
                owner: crate::sessions::claims::Owner::script(crate::sessions::AttachmentId(0)),
                recipient: crate::sessions::AttachmentId(0),
                detached: Default::default(),
                operation: lucent_protocol::operation::Operation::AcceptDrop {
                    selector: lucent_protocol::query::ItemSelector::Id(70.0),
                },
                player: crate::sessions::operations::Endpoint {
                    inbox: super::super::Inbox(sender),
                    projection: running.projection.clone(),
                    stopped: stopped.clone(),
                    packets: running.operation_packets.clone(),
                },
            }))
            .unwrap();
        assert!(matches!(
            inbox.recv_timeout(Duration::from_secs(1)).unwrap(),
            Msg::OperationAnswer(_, ScriptEvent::OperationStarted { id: 42 })
        ));
        let Msg::Command(_, reply) = inbox.recv_timeout(Duration::from_secs(1)).unwrap() else {
            panic!("expected login refresh");
        };
        reply.send(Ok("true".into())).unwrap();
        (worker, inbox, stopped)
    }

    fn prepare_drop(running: &mut Running, bridge: &Bridge) {
        extension(
            running,
            bridge,
            json!({"cmd":"dropItem","items":{"70":{
                "ItemID":70,"sName":"Dropped Item","iQty":2
            }}}),
        );
        running.drain_events();
    }

    fn loader_call(inbox: &mpsc::Receiver<Msg>) -> crate::bridge::Call {
        let Msg::Call(call) = inbox.recv_timeout(Duration::from_secs(1)).unwrap() else {
            panic!("expected drop accept");
        };
        call
    }

    fn operation_answer(inbox: &mpsc::Receiver<Msg>, timeout: Duration) -> ScriptEvent {
        let Msg::OperationAnswer(_, answer) = inbox.recv_timeout(timeout).unwrap() else {
            panic!("expected operation answer");
        };
        answer
    }

    #[test]
    fn event_overflow_keeps_the_accept_resource_until_its_packet() {
        let (mut running, bridge, _events) = running(1, 16);
        prepare_drop(&mut running, &bridge);
        let (_worker, inbox, _stopped) = accepting_operation(&mut running);
        loader_call(&inbox).resolve(Ok(Value::Bool(true)));
        queue(&mut running, &bridge, "onDebug", "first".into());
        queue(&mut running, &bridge, "onDebug", "overflow".into());
        running.drain_events();
        assert!(
            matches!(
                inbox.recv_timeout(Duration::from_millis(100)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "overflow ended a live accept"
        );
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"getDrop","ItemID":70,"bSuccess":false}),
        );
        running.drain_events();
        assert_eq!(
            operation_answer(&inbox, Duration::from_secs(1)),
            ScriptEvent::Operation {
                id: 42,
                answer: Ok(lucent_protocol::operation::OperationAnswer::AcceptDrop(
                    false
                ))
            }
        );
    }

    #[test]
    fn the_packet_wait_gets_ten_seconds_after_the_loader_returns() {
        let (mut running, bridge, _events) = running(16, 16);
        prepare_drop(&mut running, &bridge);
        let (_worker, inbox, _stopped) = accepting_operation(&mut running);
        let call = loader_call(&inbox);
        std::thread::sleep(Duration::from_secs(1));
        let returned = Instant::now();
        call.resolve(Ok(Value::Bool(true)));
        let answer = operation_answer(&inbox, Duration::from_secs(12));
        assert!(
            returned.elapsed() >= Duration::from_secs(10),
            "packet wait started before the call returned"
        );
        assert_eq!(
            answer,
            ScriptEvent::Operation {
                id: 42,
                answer: Ok(lucent_protocol::operation::OperationAnswer::AcceptDrop(
                    false
                ))
            }
        );
    }

    #[test]
    fn stopping_a_player_interrupts_a_worker_waiting_for_its_loader() {
        let (mut running, bridge, _events) = running(16, 16);
        prepare_drop(&mut running, &bridge);
        let (_worker, inbox, stopped) = accepting_operation(&mut running);
        let _hung_call = loader_call(&inbox);
        stopped.store(true, std::sync::atomic::Ordering::Release);
        running.operation_packets.stop();
        assert_eq!(
            operation_answer(&inbox, Duration::from_millis(200)),
            ScriptEvent::Operation {
                id: 42,
                answer: Err(BridgeError::Stopped)
            }
        );
    }

    #[test]
    fn a_player_stop_answers_its_waiting_accept_with_stopped() {
        let (mut running, bridge, _events) = running(16, 16);
        prepare_drop(&mut running, &bridge);
        let (_worker, inbox, stopped) = accepting_operation(&mut running);
        loader_call(&inbox).resolve(Ok(Value::Bool(true)));
        stopped.store(true, std::sync::atomic::Ordering::Release);
        running.operation_packets.stop();
        assert_eq!(
            operation_answer(&inbox, Duration::from_millis(200)),
            ScriptEvent::Operation {
                id: 42,
                answer: Err(BridgeError::Stopped)
            }
        );
    }

    #[test]
    fn dropping_operations_adds_no_loader_timeout_after_the_player_stops() {
        let (mut running, bridge, _events) = running(16, 16);
        prepare_drop(&mut running, &bridge);
        let (worker, inbox, stopped) = accepting_operation(&mut running);
        let _hung_call = loader_call(&inbox);
        stopped.store(true, std::sync::atomic::Ordering::Release);
        running.operation_packets.stop();
        let at = Instant::now();
        drop(worker);
        assert!(
            at.elapsed() < Duration::from_millis(200),
            "worker shutdown waited for the loader timeout"
        );
    }

    #[test]
    fn session_read_observes_preceding_bridge_baseline_and_delta_without_polling_events() {
        let (mut running, bridge, _events) = running(16, 16);
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"initInventory", "items":[{"ItemID":1,"iQty":3}]}),
        );
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"addItems", "items":{"1":{"ItemID":1,"iQty":2}}}),
        );
        assert_eq!(read(&mut running, |state| quantity(state, 1)), Some(5.0));
        assert!(read(&mut running, |state| state
            .current_readiness()
            .state
            .inventory));
    }

    #[test]
    fn session_unavailable_hydration_keeps_processing_following_events() {
        let (mut running, bridge, _events) = running(16, 16);
        area(&mut running, &bridge);
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"initInventory", "items":[]}),
        );
        assert_eq!(
            read(&mut running, |state| (
                state.world.get_me().is_some(),
                state.current_readiness().missing
            )),
            (
                false,
                vec![ReadinessKey::HouseInventory, ReadinessKey::Player]
            )
        );
    }

    #[test]
    fn input_overflow_clears_packet_combat_facts_before_reads_and_recovery() {
        let (mut running, bridge, _) = running(2, 16);
        running.projection.write().auth.username = "player-a".into();
        let baseline = json!({
            "cmd":"moveToArea","areaName":"battleon-42",
            "uoBranch":[{"entID":7,"uoName":"player-a","intHP":100}],
            "monBranch":[{"MonID":5,"MonMapID":1,"intHP":100,"intState":1}]
        });
        extension(&mut running, &bridge, baseline.clone());
        running.drain_events();
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"cb","a":[{
                "cmd":"aura+","tInf":"m:1","aura":{"nam":"Counter Attack","t":"s","dur":2}
            }]}),
        );
        running.drain_events();
        assert!(read(&mut running, |state| state.combat_monsters(
            now_ms(),
            &[],
            true
        )[0]
        .attack_blocked));
        for _ in 0..3 {
            queue(&mut running, &bridge, "onDebug", "overflow".into());
        }
        assert!(
            read(&mut running, |state| state.combat_monsters(
                now_ms(),
                &[],
                true
            ))
            .is_empty()
        );
        extension(&mut running, &bridge, baseline);
        assert!(!read(&mut running, |state| state.combat_monsters(
            now_ms(),
            &[],
            true
        )[0]
        .attack_blocked));
    }

    #[test]
    fn session_input_overflow_invalidates_before_reads_and_fresh_baselines_recover() {
        let (mut running, bridge, events) = running(2, 16);
        running.projection.write().auth.username = "player-a".into();
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"initInventory", "items":[{"ItemID":1,"iQty":3}]}),
        );
        assert_eq!(read(&mut running, |state| quantity(state, 1)), Some(3.0));
        assert!(
            matches!(events.try_recv().unwrap(), Some(ScriptEvent::Loader { event, .. }) if event.name == "onExtensionResponse")
        );
        area(&mut running, &bridge);
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"initInventory", "items":[]}),
        );
        queue(&mut running, &bridge, "onDebug", "overflow".into());
        assert_eq!(
            read(&mut running, |state| (
                state.receipts.epoch(),
                quantity(state, 1),
                state.current_readiness().missing
            )),
            (1, None, ReadinessKey::ALL.to_vec())
        );
        assert_eq!(events.try_recv().unwrap(), Some(ScriptEvent::Invalidated));
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"loadInventoryBig", "items":[{"ItemID":2,"iQty":9}], "hitems":[]}),
        );
        area(&mut running, &bridge);
        assert_eq!(
            read(&mut running, |state| (
                quantity(state, 2),
                state.current_readiness().missing
            )),
            (Some(9.0), vec![])
        );
    }

    #[test]
    fn session_observer_overflow_reports_loss_without_invalidating_projection() {
        let (mut running, bridge, events) = running(16, 1);
        running.projection.write().auth.username = "player-a".into();
        area(&mut running, &bridge);
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"initInventory", "items":[{"ItemID":1,"iQty":8}]}),
        );
        assert_eq!(
            read(&mut running, |state| (
                state.receipts.epoch(),
                quantity(state, 1)
            )),
            (0, Some(8.0))
        );
        assert_eq!(events.try_recv(), Err(BridgeError::Overflow));
    }

    #[test]
    fn session_server_debug_packets_do_not_duplicate_extension_mutations() {
        let (mut running, bridge, _events) = running(16, 16);
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"initInventory", "items":[{"ItemID":1,"iQty":3}]}),
        );
        queue(
            &mut running,
            &bridge,
            "packetFromServer",
            json!({"cmd":"addItems", "items":{"1":{"ItemID":1,"iQty":2}}}).to_string(),
        );
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"addItems", "items":{"1":{"ItemID":1,"iQty":2}}}),
        );
        assert_eq!(read(&mut running, |state| quantity(state, 1)), Some(5.0));
    }

    #[test]
    fn session_projection_is_isolated_between_players() {
        let (mut first, first_bridge, _first_events) = running(16, 16);
        let (mut second, second_bridge, _second_events) = running(16, 16);
        extension(
            &mut first,
            &first_bridge,
            json!({"cmd":"initInventory", "items":[{"ItemID":1,"iQty":3}]}),
        );
        extension(
            &mut second,
            &second_bridge,
            json!({"cmd":"initInventory", "items":[{"ItemID":1,"iQty":9}]}),
        );
        assert_eq!(read(&mut first, |state| quantity(state, 1)), Some(3.0));
        assert_eq!(read(&mut second, |state| quantity(state, 1)), Some(9.0));
    }

    #[test]
    fn session_reads_before_viewport_are_not_ready_and_dropped_inboxes_stop_reads() {
        let (sender, receiver) = mpsc::channel();
        let (request, reply) = Read::new(|_| 42);
        sender.send(Msg::Read(request)).unwrap();
        sender.send(Msg::Stop).unwrap();
        assert!(
            first_viewport(
                &receiver,
                false,
                &Default::default(),
                &Default::default(),
                &Default::default(),
                &Default::default(),
            )
            .is_none()
        );
        assert_eq!(
            reply.recv_timeout(Duration::ZERO),
            Err(BridgeError::NotReady)
        );
        let (request, reply) = Read::new(|_| 42);
        sender.send(Msg::Read(request)).unwrap();
        drop(receiver);
        assert_eq!(
            reply.recv_timeout(Duration::ZERO),
            Err(BridgeError::Stopped)
        );
    }

    #[test]
    fn session_read_timeout_preserves_the_queued_reply() {
        let (mut running, _bridge, _events) = running(16, 16);
        let (request, reply) = Read::new(|_| 42);
        assert_eq!(
            reply.recv_timeout(Duration::ZERO),
            Err(BridgeError::Timeout)
        );
        let _ = running.handle(Msg::Read(request));
        assert_eq!(reply.recv_timeout(Duration::ZERO), Ok(42));
    }

    #[test]
    fn panicking_read_returns_an_error_and_the_player_keeps_serving_reads() {
        let (mut running, _bridge, _events) = running(16, 16);
        let (request, reply) = Read::new::<()>(|_| panic!("read failed"));
        let _ = running.handle(Msg::Read(request));
        assert_eq!(
            reply.recv_timeout(Duration::ZERO),
            Err(BridgeError::ReadPanicked)
        );
        assert_eq!(read(&mut running, |_| 42), 42);
    }

    #[test]
    fn session_connection_loss_invalidates_preceding_baselines_and_old_world() {
        let (mut running, bridge, _events) = running(16, 16);
        running.projection.write().auth.username = "player-a".into();
        area(&mut running, &bridge);
        extension(
            &mut running,
            &bridge,
            json!({"cmd":"initInventory", "items":[]}),
        );
        assert!(read(&mut running, |state| state
            .current_readiness()
            .state
            .inventory
            && state.world.get_me().is_some()));
        queue(
            &mut running,
            &bridge,
            "onConnection",
            "OnConnectionLost".into(),
        );
        assert_eq!(
            read(&mut running, |state| (
                state.receipts.epoch(),
                state.world.get_me().is_some(),
                state.current_readiness().missing
            )),
            (1, false, ReadinessKey::ALL.to_vec())
        );
    }

    #[test]
    fn session_connection_resets_notify_projected_event_consumers() {
        let (mut running, bridge, events) = running(16, 16);
        for (status, epoch) in [
            ("OnConnection", 1),
            ("OnConnectionLost", 2),
            ("OnConnectionFailed", 3),
        ] {
            extension(
                &mut running,
                &bridge,
                json!({"cmd":"initInventory", "items":[{"ItemID":1,"iQty":3}]}),
            );
            assert_eq!(read(&mut running, |state| quantity(state, 1)), Some(3.0));
            assert!(
                matches!(events.try_recv().unwrap(), Some(ScriptEvent::Loader { event, .. }) if event.name == "onExtensionResponse")
            );
            queue(&mut running, &bridge, "onConnection", status.into());
            assert_eq!(read(&mut running, |state| quantity(state, 1)), None);
            assert_eq!(read(&mut running, |state| state.receipts.epoch()), epoch);
            assert_eq!(
                events.try_recv().unwrap(),
                Some(ScriptEvent::Loader {
                    event: Event {
                        name: "onConnection".into(),
                        args: vec![Value::String(status.into())]
                    },
                    projected: vec![],
                })
            );
            assert_eq!(events.try_recv().unwrap(), Some(ScriptEvent::Invalidated));
        }
        queue(&mut running, &bridge, "onConnection", "unknown".into());
        assert_eq!(read(&mut running, |state| state.receipts.epoch()), 3);
        assert_eq!(
            events.try_recv().unwrap(),
            Some(ScriptEvent::Loader {
                event: Event {
                    name: "onConnection".into(),
                    args: vec![Value::String("unknown".into())]
                },
                projected: vec![],
            })
        );
        assert_eq!(events.try_recv().unwrap(), None);
    }

    #[test]
    fn input_overflow_preserves_login_until_real_connection_loss() {
        for recover_before_loss in [false, true] {
            let (mut running, bridge, events) = running(32, 32);
            running.projection.write().auth.username = "player-a".into();
            area(&mut running, &bridge);
            extension(
                &mut running,
                &bridge,
                json!({"cmd":"loadInventoryBig", "items":[], "hitems":[]}),
            );
            read(&mut running, |_| ());
            let mut seen = vec![];
            while let Some(ScriptEvent::Loader { projected, .. }) = events.try_recv().unwrap() {
                seen.extend(projected.into_iter().filter(|event| {
                    matches!(event, ProjectionEvent::Login | ProjectionEvent::Logout)
                }));
            }
            assert_eq!(seen, vec![ProjectionEvent::Login]);

            for _ in 0..33 {
                queue(&mut running, &bridge, "onDebug", "overflow".into());
            }
            assert_eq!(
                read(&mut running, |state| (
                    state.receipts.epoch(),
                    state.current_readiness().missing
                )),
                (1, ReadinessKey::ALL.to_vec())
            );
            assert_eq!(events.try_recv().unwrap(), Some(ScriptEvent::Invalidated));
            assert_eq!(events.try_recv().unwrap(), None);

            if recover_before_loss {
                area(&mut running, &bridge);
                extension(
                    &mut running,
                    &bridge,
                    json!({"cmd":"loadInventoryBig", "items":[], "hitems":[]}),
                );
                assert!(read(&mut running, |state| state
                    .current_readiness()
                    .missing
                    .is_empty()));
                while let Some(ScriptEvent::Loader { projected, .. }) = events.try_recv().unwrap() {
                    seen.extend(projected.into_iter().filter(|event| {
                        matches!(event, ProjectionEvent::Login | ProjectionEvent::Logout)
                    }));
                }
                assert_eq!(seen, vec![ProjectionEvent::Login]);
            }

            queue(
                &mut running,
                &bridge,
                "onConnection",
                "OnConnectionLost".into(),
            );
            read(&mut running, |_| ());
            assert!(matches!(
                events.try_recv().unwrap(),
                Some(ScriptEvent::Loader { projected, .. }) if projected == vec![ProjectionEvent::Logout]
            ));
            assert_eq!(events.try_recv().unwrap(), Some(ScriptEvent::Invalidated));
            assert_eq!(events.try_recv().unwrap(), None);

            area(&mut running, &bridge);
            extension(
                &mut running,
                &bridge,
                json!({"cmd":"loadInventoryBig", "items":[], "hitems":[]}),
            );
            read(&mut running, |_| ());
            let mut logins = 0;
            while let Some(ScriptEvent::Loader { projected, .. }) = events.try_recv().unwrap() {
                logins += projected
                    .iter()
                    .filter(|event| **event == ProjectionEvent::Login)
                    .count();
            }
            assert_eq!(logins, 1);
        }
    }

    #[test]
    fn a_reconnect_ends_a_login_whose_connection_loss_was_lost_to_overflow() {
        for status in ["OnConnection", "OnConnectionFailed"] {
            let (mut running, bridge, events) = running(32, 64);
            running.projection.write().auth.username = "player-a".into();
            let mut sessions = vec![];
            let drain = |sessions: &mut Vec<ProjectionEvent>| {
                while let Some(event) = events.try_recv().unwrap() {
                    if let ScriptEvent::Loader { projected, .. } = event {
                        sessions.extend(projected.into_iter().filter(|event| {
                            matches!(event, ProjectionEvent::Login | ProjectionEvent::Logout)
                        }));
                    }
                }
            };
            area(&mut running, &bridge);
            extension(
                &mut running,
                &bridge,
                json!({"cmd":"loadInventoryBig", "items":[], "hitems":[]}),
            );
            read(&mut running, |_| ());
            drain(&mut sessions);

            for _ in 0..31 {
                queue(&mut running, &bridge, "onDebug", "burst".into());
            }
            queue(
                &mut running,
                &bridge,
                "onConnection",
                "OnConnectionLost".into(),
            );
            queue(&mut running, &bridge, "onDebug", "overflow".into());
            read(&mut running, |_| ());
            drain(&mut sessions);
            assert_eq!(sessions, vec![ProjectionEvent::Login]);

            queue(&mut running, &bridge, "onConnection", status.into());
            area(&mut running, &bridge);
            extension(
                &mut running,
                &bridge,
                json!({"cmd":"loadInventoryBig", "items":[], "hitems":[]}),
            );
            read(&mut running, |_| ());
            drain(&mut sessions);
            assert_eq!(
                sessions,
                vec![
                    ProjectionEvent::Login,
                    ProjectionEvent::Logout,
                    ProjectionEvent::Login
                ],
                "{status}"
            );
        }
    }

    #[test]
    fn login_and_logout_follow_readiness_and_connection_loss_once_per_session() {
        for inventory_first in [false, true] {
            let (mut running, bridge, events) = running(32, 32);
            running.projection.write().auth.username = "player-a".into();
            queue(
                &mut running,
                &bridge,
                "onConnection",
                "OnConnectionLost".into(),
            );
            read(&mut running, |_| ());
            assert!(matches!(
                events.try_recv().unwrap(),
                Some(ScriptEvent::Loader { projected, .. }) if projected.is_empty()
            ));
            assert_eq!(events.try_recv().unwrap(), Some(ScriptEvent::Invalidated));

            for _ in 0..2 {
                for inventory in [inventory_first, !inventory_first] {
                    if inventory {
                        extension(
                            &mut running,
                            &bridge,
                            json!({"cmd":"loadInventoryBig", "items":[], "hitems":[]}),
                        );
                    } else {
                        area(&mut running, &bridge);
                    }
                }
                assert!(read(&mut running, |state| state
                    .current_readiness()
                    .missing
                    .is_empty()));
                let Some(ScriptEvent::Loader { projected, .. }) = events.try_recv().unwrap() else {
                    panic!("expected the first readiness callback");
                };
                assert!(!projected.contains(&ProjectionEvent::Login));
                let Some(ScriptEvent::Loader { event, projected }) = events.try_recv().unwrap()
                else {
                    panic!("expected the readiness-completing callback");
                };
                assert_eq!(event.name, "onExtensionResponse");
                assert_eq!(projected.last(), Some(&ProjectionEvent::Login));
                assert_eq!(
                    projected
                        .iter()
                        .filter(|event| **event == ProjectionEvent::Login)
                        .count(),
                    1
                );

                area(&mut running, &bridge);
                read(&mut running, |_| ());
                assert!(matches!(
                    events.try_recv().unwrap(),
                    Some(ScriptEvent::Loader { projected, .. }) if !projected.contains(&ProjectionEvent::Login)
                ));

                queue(
                    &mut running,
                    &bridge,
                    "onConnection",
                    "OnConnectionLost".into(),
                );
                read(&mut running, |_| ());
                assert_eq!(
                    events.try_recv().unwrap(),
                    Some(ScriptEvent::Loader {
                        event: Event {
                            name: "onConnection".into(),
                            args: vec![Value::String("OnConnectionLost".into())],
                        },
                        projected: vec![ProjectionEvent::Logout],
                    })
                );
                assert_eq!(events.try_recv().unwrap(), Some(ScriptEvent::Invalidated));

                queue(
                    &mut running,
                    &bridge,
                    "onConnection",
                    "OnConnectionLost".into(),
                );
                read(&mut running, |_| ());
                assert!(matches!(
                    events.try_recv().unwrap(),
                    Some(ScriptEvent::Loader { projected, .. }) if projected.is_empty()
                ));
                assert_eq!(events.try_recv().unwrap(), Some(ScriptEvent::Invalidated));
                assert_eq!(events.try_recv().unwrap(), None);
            }
        }
    }
}
