use super::*;
use crate::game::travel;

fn config() -> Config {
    Config {
        target_name: "player-b".into(),
        ..Config::default()
    }
}
fn location(cell: &str) -> Location {
    Location {
        map: "battleon".into(),
        cell: cell.into(),
        pad: "Left".into(),
    }
}
fn player(name: &str, cell: &str) -> Player {
    Player {
        username: name.into(),
        location: location(cell),
        position: None,
    }
}
fn snapshot(target: Option<&str>) -> Snapshot {
    Snapshot {
        can_act: true,
        me: Some(player("player-a", "r1")),
        target: target.map(|cell| player("player-b", cell)),
        random_room: Some(12345),
    }
}
struct Harness {
    core: Follower,
    view: Snapshot,
    now: u64,
    combat: Option<RequestId>,
}
impl Harness {
    fn new(config: Config, view: Snapshot) -> Self {
        let mut core = Follower::new(config);
        let start = core.advance(&view, Event::Start, 0);
        let combat = start.actions.iter().find_map(|effect| match effect {
            Effect::StartCombat { id, .. } => Some(*id),
            _ => None,
        });
        Self {
            core,
            view,
            now: 0,
            combat,
        }
    }
    fn send(&mut self, event: Event) -> Effects {
        self.core.advance(&self.view, event, self.now)
    }
    fn tick(&mut self, now: u64) -> Effects {
        self.now = now;
        self.send(Event::Tick)
    }
    fn reply(&mut self, id: RequestId, reply: Reply) -> Effects {
        self.send(Event::Completed { id, reply })
    }
    fn begin_goto(&mut self) -> RequestId {
        let (id, action) = issue(self.tick(self.now));
        assert!(matches!(action, Action::Travel(travel::Request::Exit)));
        let (id, action) = issue(self.reply(id, Reply::Returned(false)));
        assert!(matches!(action, Action::GoTo(name) if name == "player-b"));
        id
    }
    fn deny(&mut self, command: &str, message: &str) -> Effects {
        self.send(Event::ServerMessage {
            command: command.into(),
            message: message.into(),
        })
    }
    fn move_target(&mut self, cell: &str, kind: LocationKind) -> Effects {
        let location = location(cell);
        if let Some(target) = &mut self.view.target {
            target.location = location.clone();
        }
        self.send(Event::TargetLocation {
            username: "PLAYER-B".into(),
            location,
            kind,
        })
    }
}
fn issue(effects: Effects) -> (RequestId, Action) {
    let mut commands = effects
        .actions
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Issue { id, action } => Some((id, action)),
            _ => None,
        });
    let command = commands.next().expect("one request");
    assert!(commands.next().is_none());
    command
}
fn join(effects: Effects, expected: &str) -> RequestId {
    let (id, action) = issue(effects);
    assert!(matches!(action, Action::Travel(travel::Request::Join {
        target, cell: None, pad: None, random_room: Some(12345),
    }) if target == expected));
    id
}
fn cancelled(effects: &Effects, id: RequestId) -> bool {
    effects
        .actions
        .iter()
        .any(|effect| matches!(effect, Effect::Cancel { id: actual } if *actual == id))
}
fn engaged(effects: &Effects, expected: bool) -> bool {
    effects.actions.iter().any(
        |effect| matches!(effect, Effect::EngageCombat { engaged, .. } if *engaged == expected),
    )
}
fn stop_count(effects: &Effects) -> usize {
    effects
        .actions
        .iter()
        .filter(|effect| matches!(effect, Effect::StopCombat { .. }))
        .count()
}
fn assert_error(h: &Harness, attempts: u32, error: &str, reason: Option<&str>) {
    let state = h.core.status();
    assert_eq!(state.attempts_remaining, attempts);
    assert_eq!(state.last_error.as_deref(), Some(error));
    assert_eq!(state.stopped_reason.as_deref(), reason);
}

#[test]
fn follows_cell_and_pad_before_engaging_combat() {
    let mut h = Harness::new(config(), snapshot(Some("r2")));
    assert_eq!(h.core.status().phase, Phase::Starting);
    let effects = h.tick(0);
    assert!(!engaged(&effects, true));
    let (id, action) = issue(effects);
    assert!(
        matches!(action, Action::Travel(travel::Request::Jump { cell, pad: Some(pad) }) if cell == "r2" && pad == "Left")
    );
    assert_eq!(h.core.status().phase, Phase::Following);
    h.view.me.as_mut().unwrap().location = location("r2");
    assert!(engaged(&h.reply(id, Reply::Returned(true)), true));
    let combat = h.combat.unwrap();
    h.send(Event::Combat {
        id: combat,
        event: CombatEvent::Attacking,
    });
    assert_eq!(h.core.status().phase, Phase::Combat);
}

#[test]
fn absent_target_exits_combat_then_gotos_and_confirms_projection() {
    let mut h = Harness::new(config(), snapshot(None));
    let id = h.begin_goto();
    let wait = h.reply(id, Reply::Returned(true));
    assert_eq!(wait.next_deadline, Some(100));
    assert_eq!(h.core.status().phase, Phase::Following);
    h.view.target = Some(player("player-b", "r1"));
    assert!(engaged(&h.tick(100), true));
}

#[test]
fn locked_and_ignored_targets_skip_goto_until_visible_again() {
    for (command, message) in [
        ("warning", " Cannot Goto Player In A Locked Zone. "),
        ("server", " PLAYER-B is ignoring goto requests. "),
    ] {
        let mut cfg = config();
        cfg.locked_zone_fallbacks = vec!["doom".into()];
        let mut h = Harness::new(cfg, snapshot(None));
        let goto = h.begin_goto();
        let effects = h.deny(command, message);
        assert!(cancelled(&effects, goto));
        let first = join(effects, "doom");
        h.reply(first, Reply::Returned(false));
        assert_error(&h, 2, "Destination room is full or unreachable", None);
        let (exit, _) = issue(h.tick(500));
        let second = join(h.reply(exit, Reply::Returned(true)), "doom");
        h.view.target = Some(player("player-b", "r1"));
        h.reply(second, Reply::Returned(true));
        h.tick(600);
        assert_eq!(h.core.status().attempts_remaining, 3);
        h.view.target = None;
        let (exit, _) = issue(h.send(Event::PlayersChanged));
        assert!(matches!(
            issue(h.reply(exit, Reply::Returned(true))).1,
            Action::GoTo(_)
        ));
    }
}

#[test]
fn fallbacks_keep_order_override_only_unsuffixed_maps_and_accept_late_room_full() {
    let mut cfg = config();
    cfg.locked_zone_fallbacks = vec![
        " doom ".into(),
        "ultradage-abc".into(),
        "ultranulgath-7".into(),
    ];
    cfg.locked_zone_room_override = " 42 ".into();
    let mut h = Harness::new(cfg, snapshot(None));
    h.begin_goto();
    let first = join(
        h.deny("warning", "Cannot goto player in a locked zone."),
        "doom-42",
    );
    let second = join(h.reply(first, Reply::Returned(false)), "ultradage-abc");
    let grace = h.reply(second, Reply::Returned(true));
    assert_eq!(grace.next_deadline, Some(100));
    h.now = 99;
    let third = join(
        h.deny("warning", "Room join failed, destination room is full."),
        "ultranulgath-7",
    );
    h.view.target = Some(player("player-b", "r2"));
    h.reply(third, Reply::Returned(true));
    assert!(h.tick(198).actions.is_empty());
    let (jump, action) = issue(h.tick(199));
    assert!(matches!(action, Action::Travel(travel::Request::Jump { cell, .. }) if cell == "r2"));
    h.view.me.as_mut().unwrap().location = location("r2");
    assert!(engaged(&h.reply(jump, Reply::Returned(true)), true));
}

#[test]
fn room_full_during_join_cancels_it_and_advances_without_spending_attempt() {
    let mut cfg = config();
    cfg.locked_zone_fallbacks = vec!["doom".into(), "dage".into()];
    let mut h = Harness::new(cfg, snapshot(None));
    h.begin_goto();
    let first = join(
        h.deny("warning", "Cannot goto player in a locked zone."),
        "doom",
    );
    let next = h.deny("warning", "Room join failed, destination room is full.");
    assert!(cancelled(&next, first));
    join(next, "dage");
    assert_eq!(h.core.status().attempts_remaining, 3);
}

#[test]
fn goto_uses_three_seconds_with_retries_and_fallbacks_and_ten_otherwise() {
    for (retry, fallbacks, timeout) in [
        (true, true, 3000),
        (true, false, 10000),
        (false, true, 10000),
        (false, false, 10000),
    ] {
        let mut cfg = config();
        cfg.retry_enabled = retry;
        if fallbacks {
            cfg.locked_zone_fallbacks = vec!["doom".into()];
        }
        let mut h = Harness::new(cfg, snapshot(None));
        let id = h.begin_goto();
        h.reply(id, Reply::Returned(true));
        assert!(h.tick(timeout - 1).actions.is_empty());
        let effects = h.tick(timeout);
        if retry && fallbacks {
            join(effects, "doom");
        } else {
            let error = if fallbacks {
                "Could not find player-b in configured locked-zone locations"
            } else {
                "Could not find player-b"
            };
            assert_error(
                &h,
                if retry { 2 } else { 0 },
                error,
                (!retry).then_some("Target not found"),
            );
        }
    }
}

#[test]
fn three_failures_wait_half_then_one_second_and_stop_with_exact_text() {
    let mut h = Harness::new(config(), snapshot(Some("r2")));
    let (first, _) = issue(h.tick(0));
    assert_eq!(
        h.reply(first, Reply::Returned(false)).next_deadline,
        Some(500)
    );
    assert_error(&h, 2, "Could not reach player-b", None);
    assert!(h.tick(499).actions.is_empty());
    let (second, _) = issue(h.tick(500));
    assert_eq!(
        h.reply(second, Reply::Returned(false)).next_deadline,
        Some(1500)
    );
    assert_error(&h, 1, "Could not reach player-b", None);
    assert!(h.tick(1499).actions.is_empty());
    let (third, _) = issue(h.tick(1500));
    let stopped = h.reply(third, Reply::Returned(false));
    assert_eq!(stop_count(&stopped), 1);
    assert_eq!(stopped.next_deadline, None);
    assert_error(
        &h,
        0,
        "Could not reach player-b",
        Some("Failed to follow target"),
    );
    assert!(!h.core.status().running);
    assert_eq!(h.core.status().phase, Phase::Stopped);
}

#[test]
fn retry_delays_cap_at_eight_seconds_and_success_resets_attempts() {
    let mut cfg = config();
    cfg.max_attempts = std::num::NonZeroU32::new(8).unwrap();
    let mut h = Harness::new(cfg, snapshot(Some("r2")));
    for delay in [500, 1000, 2000, 4000, 8000, 8000] {
        let (id, _) = issue(h.tick(h.now));
        let mut effects = h.reply(id, Reply::Returned(false));
        let retry_at = h.now + delay;
        while effects.next_deadline.unwrap() < retry_at {
            effects = h.tick(effects.next_deadline.unwrap());
            assert!(effects.actions.is_empty());
        }
        assert_eq!(effects.next_deadline, Some(retry_at));
        assert!(h.tick(retry_at - 1).actions.is_empty());
        h.now = retry_at;
    }
    let (id, _) = issue(h.tick(h.now));
    h.view.me.as_mut().unwrap().location = location("r2");
    h.reply(id, Reply::Returned(true));
    assert_eq!(h.core.status().attempts_remaining, 8);
    assert_eq!(h.core.status().last_error, None);
    h.send(Event::Stop);
    assert_eq!(h.core.status().attempts_remaining, 3);
}

#[test]
fn retry_disabled_stops_at_first_failure() {
    let mut cfg = config();
    cfg.retry_enabled = false;
    let mut h = Harness::new(cfg, snapshot(Some("r2")));
    let (id, _) = issue(h.tick(0));
    h.reply(id, Reply::Returned(false));
    assert_error(
        &h,
        0,
        "Could not reach player-b",
        Some("Failed to follow target"),
    );
    assert!(!h.core.status().enabled);
}

#[test]
fn successful_jump_must_match_the_projected_location() {
    let mut h = Harness::new(config(), snapshot(Some("r2")));
    let (id, _) = issue(h.tick(0));
    h.reply(id, Reply::Returned(true));
    assert_error(&h, 2, "Could not reach player-b", None);
}

#[test]
fn not_ready_resets_attempts_and_uses_five_second_watchdog() {
    let mut h = Harness::new(config(), snapshot(Some("r2")));
    let (id, _) = issue(h.tick(0));
    h.reply(id, Reply::Returned(false));
    h.view.can_act = false;
    h.now = 100;
    let effects = h.send(Event::ReadinessChanged);
    assert!(engaged(&effects, true));
    assert_eq!(effects.next_deadline, Some(5100));
    assert_eq!(h.core.status().attempts_remaining, 3);
    assert_eq!(h.core.status().last_error, None);
    assert!(h.tick(5099).actions.is_empty());
    assert_eq!(h.tick(5100).next_deadline, Some(10100));
}

#[test]
fn target_self_only_engages_combat_with_typed_priority_and_fallback() {
    let mut cfg = config();
    cfg.target_name = " PLAYER-A ".into();
    cfg.attack_priority = vec![AttackTarget::Id(3), AttackTarget::Name("17".into())];
    let mut core = Follower::new(cfg);
    let view = snapshot(None);
    let start = core.advance(&view, Event::Start, 0);
    assert!(
        matches!(&start.actions[..], [Effect::StartCombat { priorities, fallback_rank: 2, .. }] if priorities == &vec![AttackTarget::Id(3), AttackTarget::Name("17".into())])
    );
    let effects = core.advance(&view, Event::Tick, 0);
    assert!(engaged(&effects, true));
    assert!(
        !effects
            .actions
            .iter()
            .any(|e| matches!(e, Effect::Issue { .. }))
    );
}

#[test]
fn unrelated_players_do_not_cancel_or_charge_inflight_travel() {
    let mut h = Harness::new(config(), snapshot(None));
    let goto = h.begin_goto();
    for _ in 0..100 {
        let effects = h.send(Event::PlayersChanged);
        assert!(effects.actions.is_empty());
        assert_eq!(h.core.status().attempts_remaining, 3);
    }
    let wait = h.reply(goto, Reply::Returned(true));
    assert_eq!(wait.next_deadline, Some(100));
}

#[test]
fn target_move_cancels_travel_without_spending_attempt_and_ignores_late_reply() {
    let mut h = Harness::new(config(), snapshot(Some("r2")));
    let (old, _) = issue(h.tick(0));
    let effects = h.move_target("r3", LocationKind::Cell);
    assert!(cancelled(&effects, old));
    let (current, action) = issue(effects);
    assert_ne!(old, current);
    assert!(matches!(action, Action::Travel(travel::Request::Jump { cell, .. }) if cell == "r3"));
    assert_eq!(h.core.status().attempts_remaining, 3);
    assert!(h.reply(old, Reply::Returned(false)).actions.is_empty());
    h.view.me.as_mut().unwrap().location = location("r3");
    assert!(engaged(&h.reply(current, Reply::Returned(true)), true));
}

#[test]
fn own_join_connection_and_target_presence_changes_preempt_without_failure() {
    for event in [
        Event::OwnMapJoined,
        Event::ConnectionChanged,
        Event::PlayersChanged,
    ] {
        let mut h = Harness::new(config(), snapshot(None));
        let goto = h.begin_goto();
        h.view.target = Some(player("player-b", "r2"));
        let effects = h.send(event);
        assert!(cancelled(&effects, goto));
        assert!(matches!(
            issue(effects).1,
            Action::Travel(travel::Request::Jump { .. })
        ));
        assert_eq!(h.core.status().attempts_remaining, 3);
    }
}

#[test]
fn preempting_a_failed_relocation_keeps_attempts_and_restarts_retry_schedule() {
    let mut h = Harness::new(config(), snapshot(None));
    let goto = h.begin_goto();
    h.reply(goto, Reply::Returned(true));
    h.tick(10000);
    assert_error(&h, 2, "Could not find player-b", None);
    h.now = 10100;
    let effects = h.send(Event::OwnMapJoined);
    let (exit, _) = issue(effects);
    assert_eq!(h.core.status().attempts_remaining, 2);
    let (goto, _) = issue(h.reply(exit, Reply::Returned(true)));
    h.reply(goto, Reply::Returned(true));
    h.tick(20100);
    assert_error(&h, 1, "Could not find player-b", None);
    assert_eq!(h.tick(20100).next_deadline, Some(20600));
}

#[test]
fn copy_walk_requires_walk_event_and_cell_change_drops_pending_walk() {
    let mut cfg = config();
    cfg.copy_walk = true;
    let mut h = Harness::new(cfg, snapshot(Some("r1")));
    h.tick(0);
    let pos = Position { x: 5.0, y: 6.0 };
    assert!(
        h.move_target("r1", LocationKind::Position(pos))
            .actions
            .is_empty()
    );
    let walk = Position { x: 7.0, y: 8.0 };
    let effects = h.move_target("r1", LocationKind::Walk(Some(walk)));
    assert!(engaged(&effects, false));
    let (id, action) = issue(effects);
    assert!(matches!(action, Action::Walk(actual) if actual == walk));
    assert_eq!(h.core.status().phase, Phase::Walking);
    let change = h.move_target("r2", LocationKind::Cell);
    assert!(cancelled(&change, id));
    let (jump, _) = issue(change);
    h.view.me.as_mut().unwrap().location = location("r2");
    let effects = h.reply(jump, Reply::Returned(true));
    assert!(!effects.actions.iter().any(|effect| matches!(
        effect,
        Effect::Issue {
            action: Action::Walk(_),
            ..
        }
    )));
    assert!(h.reply(id, Reply::Returned(true)).actions.is_empty());
}

#[test]
fn newer_walk_survives_old_completion_and_invalid_positions_never_walk() {
    let mut cfg = config();
    cfg.copy_walk = true;
    let mut h = Harness::new(cfg, snapshot(Some("r1")));
    h.tick(0);
    for pos in [
        Position {
            x: f64::NAN,
            y: 1.0,
        },
        Position {
            x: 1.0,
            y: f64::INFINITY,
        },
    ] {
        assert!(
            h.move_target("r1", LocationKind::Walk(Some(pos)))
                .actions
                .is_empty()
        );
    }
    let first = Position { x: 1.0, y: 2.0 };
    let (old, _) = issue(h.move_target("r1", LocationKind::Walk(Some(first))));
    let next = Position { x: 3.0, y: 4.0 };
    let effects = h.move_target("r1", LocationKind::Walk(Some(next)));
    assert!(cancelled(&effects, old));
    let (new, _) = issue(effects);
    h.reply(old, Reply::Returned(true));
    h.reply(new, Reply::Returned(false));
    assert!(matches!(issue(h.tick(5000)).1, Action::Walk(p) if p == next));
}

#[test]
fn stop_and_reset_cancel_every_pending_operation_and_never_resume_stale_replies() {
    for operation in ["jump", "goto", "join", "walk", "exit"] {
        for reset in [false, true] {
            let mut cfg = config();
            cfg.copy_walk = true;
            cfg.locked_zone_fallbacks = vec!["doom".into()];
            let mut h = Harness::new(
                cfg,
                snapshot(if operation == "walk" {
                    Some("r1")
                } else if operation == "jump" {
                    Some("r2")
                } else {
                    None
                }),
            );
            let pending = match operation {
                "goto" => h.begin_goto(),
                "join" => {
                    h.begin_goto();
                    join(
                        h.deny("warning", "Cannot goto player in a locked zone."),
                        "doom",
                    )
                }
                "walk" => {
                    h.tick(0);
                    issue(
                        h.move_target("r1", LocationKind::Walk(Some(Position { x: 1.0, y: 2.0 }))),
                    )
                    .0
                }
                _ => issue(h.tick(0)).0,
            };
            let effects = h.send(if reset { Event::Reset } else { Event::Stop });
            assert!(cancelled(&effects, pending), "{operation}");
            assert_eq!(stop_count(&effects), 1);
            assert!(effects.actions.iter().any(|effect| matches!(
                effect,
                Effect::StopCombat { id } if *id == h.combat
            )));
            assert_eq!(effects.next_deadline, None);
            assert!(h.reply(pending, Reply::Returned(true)).actions.is_empty());
            assert!(h.tick(100000).actions.is_empty());
            assert_eq!(
                h.core.status().phase,
                if reset { Phase::Idle } else { Phase::Stopped }
            );
            assert_eq!(h.core.status().last_error, None);
            assert_eq!(h.core.status().attempts_remaining, 3);
            assert_eq!(
                h.core.status().stopped_reason.as_deref(),
                if reset { None } else { Some("Stopped by user") }
            );
            assert!(h.send(Event::Stop).actions.is_empty());
        }
    }
}

#[test]
fn reset_and_restart_keep_request_ids_distinct() {
    let mut h = Harness::new(config(), snapshot(Some("r2")));
    let (old, _) = issue(h.tick(0));
    let combat = h.combat.unwrap();
    h.send(Event::Reset);
    h.send(Event::Start);
    let (new, _) = issue(h.tick(0));
    assert_ne!(old, new);
    assert!(h.reply(old, Reply::Error("old".into())).actions.is_empty());
    h.send(Event::Combat {
        id: combat,
        event: CombatEvent::Failed("old combat".into()),
    });
    assert!(h.core.status().running);
}

#[test]
fn empty_target_has_exact_error_and_stop_text() {
    let mut h = Harness::new(Config::default(), snapshot(None));
    assert_error(&h, 3, "Target name is required", Some("Target not found"));
    assert_eq!(h.core.status().phase, Phase::Stopped);
    assert!(h.tick(0).actions.is_empty());
}

#[test]
fn combat_failures_warnings_and_dispatch_errors_follow_the_session_contract() {
    let mut h = Harness::new(config(), snapshot(Some("r1")));
    h.tick(0);
    let id = h.combat.unwrap();
    h.send(Event::Combat {
        id,
        event: CombatEvent::Warnings(vec!["warning".into()]),
    });
    h.send(Event::Combat {
        id,
        event: CombatEvent::DispatchError,
    });
    assert!(h.core.status().running);
    assert_eq!(h.core.status().last_error, None);
    h.send(Event::Combat {
        id,
        event: CombatEvent::AsyncFailure("".into()),
    });
    assert_eq!(
        h.core.status().last_error.as_deref(),
        Some("Message trigger failed")
    );
    h.send(Event::Combat {
        id,
        event: CombatEvent::Attacked,
    });
    assert_eq!(h.core.status().last_error, None);
    let stop = h.send(Event::Combat {
        id,
        event: CombatEvent::Failed("".into()),
    });
    assert_eq!(stop_count(&stop), 1);
    assert_error(
        &h,
        3,
        "Combat profile failed",
        Some("Combat profile failed"),
    );
    assert!(h.core.status().warnings.is_empty());
}

#[test]
fn goto_exception_keeps_its_message_and_empty_exception_uses_default() {
    for message in ["loader unavailable", ""] {
        let mut h = Harness::new(config(), snapshot(None));
        let id = h.begin_goto();
        h.reply(id, Reply::Error(message.into()));
        assert_error(
            &h,
            2,
            if message.is_empty() {
                "Failed to go to target"
            } else {
                message
            },
            None,
        );
    }
}

#[test]
fn disabled_core_has_no_deadlines_or_effects() {
    let mut core = Follower::new(config());
    for event in [
        Event::Tick,
        Event::PlayersChanged,
        Event::ConnectionChanged,
        Event::OwnMapJoined,
        Event::ReadinessChanged,
    ] {
        let effects = core.advance(&snapshot(None), event, 5000);
        assert!(effects.actions.is_empty());
        assert_eq!(effects.next_deadline, None);
    }
}

#[test]
fn combat_disabled_never_creates_or_engages_a_profile_job() {
    let mut cfg = config();
    cfg.combat_enabled = false;
    let mut h = Harness::new(cfg, snapshot(Some("r1")));
    assert_eq!(h.combat, None);
    let effects = h.tick(0);
    assert!(effects.actions.is_empty());
    assert_eq!(effects.next_deadline, Some(5000));
    assert_eq!(stop_count(&h.send(Event::Stop)), 1);
}

#[test]
fn location_matching_is_case_insensitive_and_pad_changes_require_travel() {
    let mut view = snapshot(Some("R1"));
    view.target.as_mut().unwrap().location.pad = "left".into();
    let mut h = Harness::new(config(), view);
    assert!(engaged(&h.tick(0), true));
    let mut destination = location("R1");
    destination.pad = "Right".into();
    h.view.target.as_mut().unwrap().location = destination.clone();
    let effects = h.send(Event::TargetLocation {
        username: "player-b".into(),
        location: destination,
        kind: LocationKind::Cell,
    });
    assert!(engaged(&effects, false));
    assert!(
        matches!(issue(effects).1, Action::Travel(travel::Request::Jump { pad: Some(pad), .. }) if pad == "Right")
    );
}

#[test]
fn target_map_change_preempts_even_when_cell_and_pad_match() {
    let mut h = Harness::new(config(), snapshot(Some("r2")));
    let (old, _) = issue(h.tick(0));
    let mut target = location("r2");
    target.map = "doom".into();
    h.view.target.as_mut().unwrap().location = target.clone();
    let effects = h.send(Event::TargetLocation {
        username: "player-b".into(),
        location: target,
        kind: LocationKind::Cell,
    });
    assert!(cancelled(&effects, old));
    assert_eq!(h.core.status().attempts_remaining, 3);
}

#[test]
fn other_players_and_duplicate_walks_do_not_wake_the_follower() {
    let mut cfg = config();
    cfg.copy_walk = true;
    let mut h = Harness::new(cfg, snapshot(Some("r1")));
    h.tick(0);
    assert!(
        h.send(Event::TargetLocation {
            username: "other".into(),
            location: location("r3"),
            kind: LocationKind::Cell
        })
        .actions
        .is_empty()
    );
    let pos = Position { x: 1.0, y: 2.0 };
    let (id, _) = issue(h.move_target("r1", LocationKind::Walk(Some(pos))));
    assert!(
        h.move_target("r1", LocationKind::Walk(Some(pos)))
            .actions
            .is_empty()
    );
    h.reply(id, Reply::Returned(true));
    assert!(h.tick(5000).actions.is_empty());
}

#[test]
fn room_full_after_grace_does_not_reject_a_successful_fallback() {
    let mut cfg = config();
    cfg.locked_zone_fallbacks = vec!["doom".into()];
    let mut h = Harness::new(cfg, snapshot(None));
    h.begin_goto();
    let id = join(
        h.deny("warning", "Cannot goto player in a locked zone."),
        "doom",
    );
    h.view.target = Some(player("player-b", "r1"));
    h.reply(id, Reply::Returned(true));
    h.tick(100);
    h.now = 101;
    assert!(
        h.deny("warning", "Room join failed, destination room is full.")
            .actions
            .is_empty()
    );
    assert!(h.core.status().running);
    assert_eq!(h.core.status().last_error, None);
}

#[test]
fn unrelated_warnings_never_deny_the_target_or_skip_a_fallback() {
    let mut h = Harness::new(config(), snapshot(None));
    let id = h.begin_goto();
    for (command, message) in [
        ("warning", "other is ignoring goto requests."),
        ("server", "other is ignoring goto requests."),
        ("server", "Cannot goto player in a locked zone."),
        ("warning", "Room join failed, destination room is full."),
    ] {
        assert!(h.deny(command, message).actions.is_empty());
    }
    assert_eq!(h.reply(id, Reply::Returned(true)).next_deadline, Some(100));
}

#[test]
fn goto_final_projection_check_wins_over_a_denial() {
    let mut h = Harness::new(config(), snapshot(None));
    let goto = h.begin_goto();
    h.view.target = Some(player("player-b", "r1"));
    let effects = h.deny("warning", "Cannot goto player in a locked zone.");
    assert!(cancelled(&effects, goto));
    assert!(engaged(&effects, true));
    assert_eq!(h.core.status().last_error, None);
}

#[test]
fn readiness_edge_waits_for_inflight_travel_without_canceling_it() {
    let mut h = Harness::new(config(), snapshot(Some("r2")));
    let (id, _) = issue(h.tick(0));
    h.view.can_act = false;
    assert!(h.send(Event::ReadinessChanged).actions.is_empty());
    assert_eq!(h.core.status().attempts_remaining, 3);
    h.view.me.as_mut().unwrap().location = location("r2");
    h.reply(id, Reply::Returned(true));
    assert_eq!(h.core.status().last_error, None);
}

#[test]
fn raw_goto_timeout_cancels_the_request_and_ignores_its_late_reply() {
    for late_reply in [false, true] {
        let mut h = Harness::new(config(), snapshot(None));
        let id = h.begin_goto();
        assert!(h.tick(4999).actions.is_empty());
        h.now = 5000;
        let effects = if late_reply {
            h.reply(id, Reply::Returned(true))
        } else {
            h.tick(5000)
        };
        assert!(cancelled(&effects, id));
        assert_error(&h, 2, "Failed to go to target", None);
        assert!(h.reply(id, Reply::Returned(true)).actions.is_empty());
    }
}

#[test]
fn explicit_stop_after_exhaustion_clears_the_error_and_reports_user_stop() {
    let mut cfg = config();
    cfg.retry_enabled = false;
    let mut h = Harness::new(cfg, snapshot(Some("r2")));
    let (id, _) = issue(h.tick(0));
    h.reply(id, Reply::Returned(false));
    let effects = h.send(Event::Stop);
    assert_eq!(stop_count(&effects), 0);
    assert_eq!(
        h.core.status().stopped_reason.as_deref(),
        Some("Stopped by user")
    );
    assert_eq!(h.core.status().last_error, None);
    assert_eq!(h.core.status().attempts_remaining, 3);
}

#[test]
fn non_ascii_cell_names_deliberately_do_not_use_locale_collation() {
    let mut view = snapshot(Some("Ä"));
    view.me.as_mut().unwrap().location.cell = "ä".into();
    let mut h = Harness::new(config(), view);
    let (_, action) = issue(h.tick(0));
    assert!(matches!(action, Action::Travel(travel::Request::Jump { cell, .. }) if cell == "Ä"));
}

#[test]
fn configuration_changes_only_while_inactive_and_keeps_request_ids_distinct() {
    let mut h = Harness::new(config(), snapshot(None));
    let old = h.begin_goto();
    let mut replacement = config();
    replacement.target_name = " PLAYER-C ".into();
    replacement.max_attempts = std::num::NonZeroU32::new(5).unwrap();
    assert!(!h.core.configure(replacement.clone()));
    h.reply(old, Reply::Error("loader unavailable".into()));
    let (exit, _) = issue(h.tick(500));
    let (_, action) = issue(h.reply(exit, Reply::Returned(true)));
    assert!(matches!(action, Action::GoTo(name) if name == "player-b"));
    h.send(Event::Stop);
    assert!(h.core.configure(replacement));
    assert_eq!(h.core.status().last_error, None);
    assert_eq!(h.core.status().stopped_reason, None);
    h.send(Event::Start);
    assert_eq!(h.core.status().attempts_remaining, 5);
    let (exit, _) = issue(h.tick(h.now));
    let (new, action) = issue(h.reply(exit, Reply::Returned(true)));
    assert!(matches!(action, Action::GoTo(name) if name == "player-c"));
    assert_ne!(old, new);
    assert!(
        h.reply(old, Reply::Error("stale".into()))
            .actions
            .is_empty()
    );
    assert_eq!(h.core.status().attempts_remaining, 5);
}
