use super::queue::QueueRunStatus;
use super::*;

fn options() -> Options {
    Options {
        room_policy: RoomPolicy::RandomPrivate,
        safe_start_stop: false,
        restart_after_reconnect: false,
    }
}
fn readiness() -> Readiness {
    Readiness {
        logged_in: true,
        username: "tommy".into(),
        projections_ready: true,
    }
}
fn binding() -> Binding {
    Binding {
        player: PlayerId(1),
        connection: ConnectionEpoch(1),
    }
}
pub(super) fn request() -> Request {
    use lucent_protocol::script::{ModulePath, Script};
    let path = ModulePath::loose("test.js").unwrap();
    let script = Script::new(
        path.clone(),
        [(path, "export default function*() {}".into())].into(),
        Default::default(),
    )
    .unwrap();
    Request {
        name: "test".into(),
        reference: "test.js".into(),
        script: std::sync::Arc::new(script),
        inputs: Default::default(),
    }
}
fn send(machine: &mut Lifecycle, event: Event, now: u64) -> Transition {
    machine.advance(event, &readiness(), &options(), now)
}
fn start(machine: &mut Lifecycle) -> RunId {
    let result = send(machine, Event::Command(Command::Start(request())), 0);
    assert!(result.effects.contains(&Effect::ScheduleDeadline(10_000)));
    let run = machine.status().active.unwrap().run;
    let read = send(machine, Event::PlayerReady { run, ready: true }, 1);
    assert!(
        read.effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. }))
    );
    send(machine, Event::RunReady(run), 2);
    run
}
#[test]
fn stop_deadline_forces_one_stopped_outcome() {
    let mut machine = Lifecycle::new(binding());
    let run = start(&mut machine);
    let stopped = send(
        &mut machine,
        Event::Command(Command::Stop {
            run,
            reason: "user".into(),
        }),
        100,
    );
    assert!(stopped.effects.contains(&Effect::SignalStop { run }));
    assert!(stopped.effects.contains(&Effect::ScheduleDeadline(5100)));
    assert!(
        !send(&mut machine, Event::Deadline, 5099)
            .effects
            .contains(&Effect::Kill { run })
    );
    let expired = send(&mut machine, Event::Deadline, 5100);
    assert!(expired.effects.contains(&Effect::Kill { run }));
    assert_eq!(
        terminals(&expired),
        vec![Outcome::Stopped(StopReason::GraceExpired)]
    );
    assert!(
        terminals(&send(
            &mut machine,
            Event::RunExited {
                run,
                outcome: Outcome::Completed
            },
            5101
        ))
        .is_empty()
    );
}
fn terminals(result: &Transition) -> Vec<Outcome> {
    result
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Terminal(record) => Some(record.outcome.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn every_ending_is_published_once_and_reaped() {
    for outcome in [
        Outcome::Completed,
        Outcome::Failed("invalid inputs".into()),
        Outcome::Stopped(StopReason::Script(None)),
        Outcome::Exited {
            logout: false,
            close_client: true,
        },
        Outcome::Killed(KillReason::Watchdog),
        Outcome::Killed(KillReason::Heap),
    ] {
        let mut machine = Lifecycle::new(binding());
        let run = start(&mut machine);
        let event = Event::RunExited {
            run,
            outcome: outcome.clone(),
        };
        let result = send(&mut machine, event.clone(), 10);
        assert_eq!(terminals(&result), vec![outcome.clone()]);
        assert!(result.effects.contains(&Effect::Revoke { run }));
        assert!(result.effects.contains(&Effect::Reap { run }));
        assert!(terminals(&send(&mut machine, event, 11)).is_empty());
        send(&mut machine, Event::Reaped(run), 12);
        assert!(machine.status().active.is_none());
        assert_eq!(machine.status().latest.unwrap().outcome, outcome);
    }
}
#[test]
fn stale_controls_and_callbacks_cannot_touch_a_new_run() {
    let mut machine = Lifecycle::new(binding());
    let old = start(&mut machine);
    send(
        &mut machine,
        Event::RunExited {
            run: old,
            outcome: Outcome::Completed,
        },
        3,
    );
    send(&mut machine, Event::Reaped(old), 4);
    let run = start(&mut machine);
    assert_ne!(old, run);
    for event in [
        Event::Command(Command::Stop {
            run: old,
            reason: "stale".into(),
        }),
        Event::Command(Command::Kill { run: old }),
        Event::RunReady(old),
        Event::RunExited {
            run: old,
            outcome: Outcome::Failed("late".into()),
        },
        Event::Reaped(old),
        Event::PlayerReady {
            run: old,
            ready: true,
        },
        Event::SafeMoveDone {
            run: old,
            phase: SafePhase::After,
        },
    ] {
        assert!(send(&mut machine, event, 20).effects.is_empty());
    }
    assert_eq!(machine.status().active.unwrap().run, run);
}
#[test]
fn stop_then_kill_is_immediate_and_repeated_stop_does_not_extend_grace() {
    let mut machine = Lifecycle::new(binding());
    let run = start(&mut machine);
    send(
        &mut machine,
        Event::Command(Command::Stop {
            run,
            reason: "first".into(),
        }),
        10,
    );
    assert!(
        send(
            &mut machine,
            Event::Command(Command::Stop {
                run,
                reason: "again".into()
            }),
            100
        )
        .effects
        .is_empty()
    );
    let killed = send(&mut machine, Event::Command(Command::Kill { run }), 101);
    assert!(killed.effects.contains(&Effect::Kill { run }));
    assert_eq!(
        terminals(&killed),
        vec![Outcome::Killed(KillReason::Requested)]
    );
    assert!(terminals(&send(&mut machine, Event::Deadline, 5010)).is_empty());
}
#[test]
fn graceful_stop_overrides_completion_but_preserves_watchdog_and_heap() {
    for exit in [
        Outcome::Completed,
        Outcome::Failed("cleanup failed".into()),
        Outcome::Killed(KillReason::Watchdog),
        Outcome::Killed(KillReason::Heap),
    ] {
        let mut machine = Lifecycle::new(binding());
        let run = start(&mut machine);
        send(
            &mut machine,
            Event::Command(Command::Stop {
                run,
                reason: "user".into(),
            }),
            10,
        );
        let result = send(
            &mut machine,
            Event::RunExited {
                run,
                outcome: exit.clone(),
            },
            11,
        );
        let expected = if matches!(exit, Outcome::Killed(_)) {
            exit
        } else {
            Outcome::Stopped(StopReason::Requested("user".into()))
        };
        assert_eq!(terminals(&result), vec![expected]);
    }
}
#[test]
fn reconnect_uses_a_fresh_run_only_when_enabled() {
    for enabled in [false, true] {
        let mut machine = Lifecycle::new(binding());
        let run = start(&mut machine);
        let mut opts = options();
        opts.restart_after_reconnect = enabled;
        let lost = machine.advance(Event::ConnectionLost(binding()), &readiness(), &opts, 10);
        assert_eq!(
            terminals(&lost),
            vec![Outcome::Stopped(StopReason::ConnectionLost)]
        );
        assert!(lost.effects.contains(&Effect::Kill { run }));
        assert_eq!(machine.status().waiting_to_restart, enabled);
        machine.advance(Event::Reaped(run), &readiness(), &opts, 11);
        let rebound = Binding {
            connection: ConnectionEpoch(2),
            ..binding()
        };
        machine.advance(Event::ConnectionRestored(rebound), &readiness(), &opts, 20);
        if enabled {
            assert_ne!(machine.status().active.unwrap().run, run);
        } else {
            assert!(machine.status().active.is_none());
        }
    }
}
#[test]
fn account_change_option_disable_and_player_replacement_cancel_restart() {
    for event in [
        Event::AccountChanged,
        Event::OptionsChanged,
        Event::PlayerReplaced(Binding {
            player: PlayerId(2),
            connection: ConnectionEpoch(1),
        }),
    ] {
        let mut machine = Lifecycle::new(binding());
        let run = start(&mut machine);
        let opts = Options {
            restart_after_reconnect: true,
            ..options()
        };
        machine.advance(Event::ConnectionLost(binding()), &readiness(), &opts, 10);
        assert!(machine.status().waiting_to_restart);
        let opts = if matches!(event, Event::OptionsChanged) {
            options()
        } else {
            opts
        };
        machine.advance(event, &readiness(), &opts, 11);
        assert!(!machine.status().waiting_to_restart);
        machine.advance(Event::Reaped(run), &readiness(), &opts, 12);
        machine.advance(
            Event::ConnectionRestored(binding()),
            &readiness(),
            &opts,
            13,
        );
        assert!(machine.status().active.is_none());
    }
}
#[test]
fn player_replacement_ends_the_run_without_restart() {
    let mut machine = Lifecycle::new(binding());
    let run = start(&mut machine);
    let opts = Options {
        restart_after_reconnect: true,
        ..options()
    };
    let result = machine.advance(
        Event::PlayerReplaced(Binding {
            player: PlayerId(2),
            ..binding()
        }),
        &readiness(),
        &opts,
        10,
    );
    assert_eq!(
        terminals(&result),
        vec![Outcome::Stopped(StopReason::PlayerReplaced)]
    );
    assert!(result.effects.contains(&Effect::Kill { run }));
    assert!(!machine.status().waiting_to_restart);
}
#[test]
fn safe_start_boot_outcome_safe_stop_and_logout_are_ordered() {
    let mut machine = Lifecycle::new(binding());
    let opts = Options {
        safe_start_stop: true,
        ..options()
    };
    machine.advance(
        Event::Command(Command::Start(request())),
        &readiness(),
        &opts,
        0,
    );
    let run = machine.status().active.unwrap().run;
    let before = machine.advance(
        Event::PlayerReady { run, ready: true },
        &readiness(),
        &opts,
        1,
    );
    assert!(before.effects.iter().any(|e| matches!(
        e,
        Effect::SafeMove {
            phase: SafePhase::Before,
            ..
        }
    )));
    assert!(
        !before
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Spawn { .. }))
    );
    let boot = machine.advance(
        Event::SafeMoveDone {
            run,
            phase: SafePhase::Before,
        },
        &readiness(),
        &opts,
        2,
    );
    assert!(
        boot.effects
            .iter()
            .any(|e| matches!(e, Effect::Spawn { .. }))
    );
    let result = machine.advance(
        Event::RunExited {
            run,
            outcome: Outcome::Exited {
                logout: true,
                close_client: true,
            },
        },
        &readiness(),
        &opts,
        3,
    );
    let terminal = result
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Terminal(_)))
        .unwrap();
    let after = result
        .effects
        .iter()
        .position(|e| {
            matches!(
                e,
                Effect::SafeMove {
                    phase: SafePhase::After,
                    ..
                }
            )
        })
        .unwrap();
    assert!(terminal < after);
    assert!(
        !result
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Logout { .. }))
    );
    machine.advance(Event::Reaped(run), &readiness(), &opts, 4);
    assert!(machine.status().active.is_some());
    let logout = machine.advance(
        Event::SafeMoveDone {
            run,
            phase: SafePhase::After,
        },
        &readiness(),
        &opts,
        5,
    );
    assert!(logout.effects.contains(&Effect::Logout {
        run,
        binding: binding()
    }));
    machine.advance(Event::LogoutDone(run), &readiness(), &opts, 6);
    assert!(machine.status().active.is_none());
}
#[test]
fn forced_kill_still_runs_safe_stop_and_reaping_cannot_skip_it() {
    let mut machine = Lifecycle::new(binding());
    let run = start(&mut machine);
    let opts = Options {
        safe_start_stop: true,
        ..options()
    };
    machine.advance(
        Event::Command(Command::Stop {
            run,
            reason: "user".into(),
        }),
        &readiness(),
        &opts,
        10,
    );
    let result = machine.advance(Event::Deadline, &readiness(), &opts, 5010);
    assert_eq!(
        terminals(&result),
        vec![Outcome::Stopped(StopReason::GraceExpired)]
    );
    let terminal = result
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Terminal(_)))
        .unwrap();
    let after = result
        .effects
        .iter()
        .position(|e| {
            matches!(
                e,
                Effect::SafeMove {
                    phase: SafePhase::After,
                    ..
                }
            )
        })
        .unwrap();
    assert!(terminal < after);
    machine.advance(
        Event::SafeMoveDone {
            run,
            phase: SafePhase::After,
        },
        &readiness(),
        &opts,
        5011,
    );
    assert!(machine.status().active.is_some());
    machine.advance(Event::Reaped(run), &readiness(), &opts, 5012);
    assert!(machine.status().active.is_none());
}
#[test]
fn readiness_waits_for_a_pending_read_within_the_ten_second_bound() {
    let mut machine = Lifecycle::new(binding());
    let not_ready = Readiness::default();
    let result = machine.advance(
        Event::Command(Command::Start(request())),
        &not_ready,
        &options(),
        50,
    );
    assert!(result.effects.contains(&Effect::ScheduleDeadline(10050)));
    assert!(
        !result
            .effects
            .iter()
            .any(|e| matches!(e, Effect::ReadPlayerReady { .. }))
    );
    let run = machine.status().active.unwrap().run;
    let read = send(&mut machine, Event::ProjectionChanged, 51);
    assert!(read.effects.contains(&Effect::ReadPlayerReady {
        run,
        binding: binding(),
        deadline: 10050
    }));
    assert!(
        send(&mut machine, Event::ProjectionChanged, 52)
            .effects
            .is_empty()
    );
    assert!(
        send(&mut machine, Event::Deadline, 10049)
            .effects
            .is_empty()
    );
    let result = send(&mut machine, Event::PlayerReady { run, ready: true }, 10050);
    assert_eq!(
        terminals(&result),
        vec![Outcome::Failed(
            "Script readiness timed out after 10 s".into()
        )]
    );
    assert!(
        !result
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Spawn { .. }))
    );
}
#[test]
fn starts_are_refused_while_a_worker_is_reaping() {
    let mut machine = Lifecycle::new(binding());
    let run = start(&mut machine);
    send(
        &mut machine,
        Event::RunExited {
            run,
            outcome: Outcome::Completed,
        },
        10,
    );
    assert_eq!(
        send(&mut machine, Event::Command(Command::Start(request())), 11).rejection,
        Some(Rejection::Busy)
    );
    send(&mut machine, Event::Reaped(run), 12);
    assert_eq!(
        send(&mut machine, Event::Command(Command::Start(request())), 13).rejection,
        None
    );
}

fn queued_pair(machine: &mut Lifecycle, opts: &Options) -> RunId {
    for id in 1..=2 {
        let entry = Entry {
            id: EntryId(id),
            request: request(),
            inputs_available: false,
        };
        machine.advance(
            Event::Command(Command::Queue(QueueCommand::Add(entry))),
            &readiness(),
            opts,
            0,
        );
    }
    let mut effects = machine
        .advance(
            Event::Command(Command::Queue(QueueCommand::Start)),
            &readiness(),
            opts,
            1,
        )
        .effects;
    for _ in 0..2 {
        let (id, entry) = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::Prepare { id, entry } => Some((*id, entry.clone())),
                _ => None,
            })
            .unwrap();
        effects = machine
            .advance(
                Event::Prepared {
                    id,
                    result: Ok(entry),
                },
                &readiness(),
                opts,
                2,
            )
            .effects;
    }
    let run = machine.status().active.unwrap().run;
    machine.advance(
        Event::PlayerReady { run, ready: true },
        &readiness(),
        opts,
        3,
    );
    if opts.safe_start_stop {
        machine.advance(
            Event::SafeMoveDone {
                run,
                phase: SafePhase::Before,
            },
            &readiness(),
            opts,
            4,
        );
    }
    machine.advance(Event::RunReady(run), &readiness(), opts, 5);
    run
}
#[test]
fn queued_next_waits_for_both_safe_stop_and_reaping() {
    for reap_first in [false, true] {
        let mut machine = Lifecycle::new(binding());
        let opts = Options {
            safe_start_stop: true,
            ..options()
        };
        let run = queued_pair(&mut machine, &opts);
        machine.advance(
            Event::RunExited {
                run,
                outcome: Outcome::Completed,
            },
            &readiness(),
            &opts,
            10,
        );
        let reaped = Event::Reaped(run);
        let safe_done = Event::SafeMoveDone {
            run,
            phase: SafePhase::After,
        };
        let (first, second) = if reap_first {
            (reaped, safe_done)
        } else {
            (safe_done, reaped)
        };
        let one = machine.advance(first, &readiness(), &opts, 11);
        assert!(
            !one.effects
                .iter()
                .any(|e| matches!(e, Effect::ReadPlayerReady { .. } | Effect::Spawn { .. }))
        );
        assert_eq!(machine.status().active.unwrap().run, run);
        let two = machine.advance(second, &readiness(), &opts, 12);
        assert!(
            two.effects
                .iter()
                .any(|e| matches!(e, Effect::ReadPlayerReady { run: next, .. } if *next != run))
        );
        assert_ne!(machine.status().active.unwrap().run, run);
    }
}
#[test]
fn reconnect_keeps_the_queue_item_until_its_fresh_run_completes() {
    let mut machine = Lifecycle::new(binding());
    let opts = Options {
        restart_after_reconnect: true,
        ..options()
    };
    let run = queued_pair(&mut machine, &opts);
    machine.advance(Event::ConnectionLost(binding()), &readiness(), &opts, 10);
    assert_eq!(machine.status().queue.current, Some(0));
    machine.advance(Event::Reaped(run), &readiness(), &opts, 11);
    let rebound = Binding {
        connection: ConnectionEpoch(2),
        ..binding()
    };
    machine.advance(Event::ConnectionRestored(rebound), &readiness(), &opts, 12);
    let next = machine.status().active.unwrap().run;
    assert_ne!(run, next);
    let spawn = machine.advance(
        Event::PlayerReady {
            run: next,
            ready: true,
        },
        &readiness(),
        &opts,
        13,
    );
    assert!(
        spawn
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Spawn { binding, .. } if *binding == rebound))
    );
    machine.advance(
        Event::RunExited {
            run: next,
            outcome: Outcome::Completed,
        },
        &readiness(),
        &opts,
        14,
    );
    assert_eq!(machine.status().queue.current, Some(1));
}
#[test]
fn disabling_restart_settles_the_disconnected_queue_item() {
    let mut machine = Lifecycle::new(binding());
    let opts = Options {
        restart_after_reconnect: true,
        ..options()
    };
    let run = queued_pair(&mut machine, &opts);
    machine.advance(Event::ConnectionLost(binding()), &readiness(), &opts, 10);
    send(&mut machine, Event::OptionsChanged, 11);
    assert_eq!(
        machine.status().queue.latest.unwrap().status,
        QueueRunStatus::Stopped
    );
    send(&mut machine, Event::Reaped(run), 12);
    send(&mut machine, Event::ConnectionRestored(binding()), 13);
    assert!(machine.status().active.is_none());
}
#[test]
fn queue_cancellation_during_reaping_prevents_the_next_spawn() {
    let mut machine = Lifecycle::new(binding());
    let run = queued_pair(&mut machine, &options());
    send(
        &mut machine,
        Event::RunExited {
            run,
            outcome: Outcome::Completed,
        },
        10,
    );
    send(
        &mut machine,
        Event::Command(Command::Queue(QueueCommand::Cancel("user".into()))),
        11,
    );
    send(&mut machine, Event::Reaped(run), 12);
    assert!(machine.status().active.is_none());
    assert_eq!(
        machine.status().queue.latest.unwrap().status,
        QueueRunStatus::Canceled
    );
}
#[test]
fn readiness_rejects_missing_facts_and_false_or_stale_loader_answers() {
    for readiness in [
        Readiness {
            logged_in: false,
            ..readiness()
        },
        Readiness {
            username: "  ".into(),
            ..readiness()
        },
        Readiness {
            projections_ready: false,
            ..readiness()
        },
    ] {
        let mut machine = Lifecycle::new(binding());
        let result = machine.advance(
            Event::Command(Command::Start(request())),
            &readiness,
            &options(),
            0,
        );
        assert!(
            !result
                .effects
                .iter()
                .any(|e| matches!(e, Effect::ReadPlayerReady { .. }))
        );
        let run = machine.status().active.unwrap().run;
        let result = machine.advance(
            Event::PlayerReady { run, ready: true },
            &readiness,
            &options(),
            1,
        );
        assert!(
            !result
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Spawn { .. }))
        );
    }
    let mut machine = Lifecycle::new(binding());
    send(&mut machine, Event::Command(Command::Start(request())), 0);
    let run = machine.status().active.unwrap().run;
    assert!(
        !send(&mut machine, Event::PlayerReady { run, ready: false }, 1)
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Spawn { .. }))
    );
    assert!(
        send(&mut machine, Event::PlayerReady { run, ready: true }, 2)
            .effects
            .is_empty()
    );
    assert_eq!(
        terminals(&send(&mut machine, Event::Deadline, 10_000)),
        vec![Outcome::Failed(
            "Script readiness timed out after 10 s".into()
        )]
    );
}
#[test]
fn stop_before_boot_cancels_safe_start_and_never_spawns() {
    let mut machine = Lifecycle::new(binding());
    let opts = Options {
        safe_start_stop: true,
        ..options()
    };
    machine.advance(
        Event::Command(Command::Start(request())),
        &readiness(),
        &opts,
        0,
    );
    let run = machine.status().active.unwrap().run;
    machine.advance(
        Event::PlayerReady { run, ready: true },
        &readiness(),
        &opts,
        1,
    );
    let stopped = machine.advance(
        Event::Command(Command::Stop {
            run,
            reason: "user".into(),
        }),
        &readiness(),
        &opts,
        2,
    );
    assert!(stopped.effects.contains(&Effect::CancelSafeMove {
        run,
        phase: SafePhase::Before
    }));
    assert!(!stopped.effects.contains(&Effect::Reap { run }));
    assert!(!stopped.effects.iter().any(|e| matches!(
        e,
        Effect::SafeMove {
            phase: SafePhase::After,
            ..
        }
    )));
    let late = machine.advance(
        Event::SafeMoveDone {
            run,
            phase: SafePhase::Before,
        },
        &readiness(),
        &opts,
        3,
    );
    assert!(
        !late
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Spawn { .. }))
    );
}
#[test]
fn old_connection_events_do_not_kill_a_reconnected_run() {
    let mut machine = Lifecycle::new(binding());
    let rebound = Binding {
        connection: ConnectionEpoch(2),
        ..binding()
    };
    send(&mut machine, Event::ConnectionRestored(rebound), 0);
    let run = start(&mut machine);
    assert!(
        send(&mut machine, Event::ConnectionLost(binding()), 10)
            .effects
            .is_empty()
    );
    assert_eq!(machine.status().active.unwrap().run, run);
}
#[test]
fn queue_capacity_and_duplicate_ids_are_rejected() {
    let mut machine = Lifecycle::new(binding());
    let entry = Entry {
        id: EntryId(1),
        request: request(),
        inputs_available: false,
    };
    send(
        &mut machine,
        Event::Command(Command::Queue(QueueCommand::Add(entry.clone()))),
        0,
    );
    assert_eq!(
        send(
            &mut machine,
            Event::Command(Command::Queue(QueueCommand::Add(entry))),
            1
        )
        .rejection,
        Some(Rejection::DuplicateEntry)
    );
    for id in 2..=256 {
        assert_eq!(
            send(
                &mut machine,
                Event::Command(Command::Queue(QueueCommand::Add(Entry {
                    id: EntryId(id),
                    request: request(),
                    inputs_available: false
                }))),
                0
            )
            .rejection,
            None
        );
    }
    assert_eq!(
        send(
            &mut machine,
            Event::Command(Command::Queue(QueueCommand::Add(Entry {
                id: EntryId(257),
                request: request(),
                inputs_available: false
            }))),
            0
        )
        .rejection,
        Some(Rejection::Full)
    );
}

#[test]
fn account_change_waits_for_canceled_safe_stop_without_logging_out() {
    let mut machine = Lifecycle::new(binding());
    let run = start(&mut machine);
    let opts = Options {
        safe_start_stop: true,
        ..options()
    };
    machine.advance(
        Event::RunExited {
            run,
            outcome: Outcome::Exited {
                logout: true,
                close_client: false,
            },
        },
        &readiness(),
        &opts,
        10,
    );
    machine.advance(Event::Reaped(run), &readiness(), &opts, 11);
    let canceled = machine.advance(Event::AccountChanged, &readiness(), &opts, 12);
    assert!(canceled.effects.contains(&Effect::CancelSafeMove {
        run,
        phase: SafePhase::After
    }));
    assert!(machine.status().active.is_some());
    let done = machine.advance(
        Event::SafeMoveDone {
            run,
            phase: SafePhase::After,
        },
        &readiness(),
        &opts,
        13,
    );
    assert!(
        !done
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Logout { .. }))
    );
    assert!(machine.status().active.is_none());
}

#[test]
fn starts_are_refused_while_a_run_is_active() {
    let mut machine = Lifecycle::new(binding());
    let run = start(&mut machine);
    for command in [
        Command::Start(request()),
        Command::Queue(QueueCommand::Start),
    ] {
        assert_eq!(
            send(&mut machine, Event::Command(command), 10).rejection,
            Some(Rejection::Busy)
        );
        assert_eq!(machine.status().active.unwrap().run, run);
    }
}

#[test]
fn disabling_restart_while_the_loader_read_is_pending_cancels_the_attempt() {
    for loader_reply in [false, true] {
        let mut machine = Lifecycle::new(binding());
        let run = start(&mut machine);
        let opts = Options {
            restart_after_reconnect: true,
            ..options()
        };
        machine.advance(Event::ConnectionLost(binding()), &readiness(), &opts, 10);
        machine.advance(Event::Reaped(run), &readiness(), &opts, 11);
        machine.advance(
            Event::ConnectionRestored(Binding {
                connection: ConnectionEpoch(2),
                ..binding()
            }),
            &readiness(),
            &opts,
            12,
        );
        let retry = machine.status().active.unwrap().run;
        assert_ne!(run, retry);
        let event = if loader_reply {
            Event::PlayerReady {
                run: retry,
                ready: true,
            }
        } else {
            Event::OptionsChanged
        };
        let canceled = send(&mut machine, event, 13);
        assert_eq!(
            terminals(&canceled),
            vec![Outcome::Stopped(StopReason::Requested(
                "Restart after reconnect disabled".into()
            ))]
        );
        assert!(machine.status().active.is_none());
        assert!(!machine.status().waiting_to_restart);
        assert!(
            !send(
                &mut machine,
                Event::PlayerReady {
                    run: retry,
                    ready: true
                },
                14
            )
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Spawn { .. }))
        );
    }
}

#[test]
fn reconnect_compares_normalized_account_names() {
    for (username, restart) in [("ÉOWYN", true), ("other", false)] {
        let mut machine = Lifecycle::new(binding());
        let opts = Options {
            restart_after_reconnect: true,
            ..options()
        };
        let account = Readiness {
            username: "Éowyn".into(),
            ..readiness()
        };
        machine.advance(
            Event::Command(Command::Start(request())),
            &account,
            &opts,
            0,
        );
        let run = machine.status().active.unwrap().run;
        machine.advance(Event::PlayerReady { run, ready: true }, &account, &opts, 1);
        machine.advance(Event::ConnectionLost(binding()), &account, &opts, 2);
        machine.advance(Event::Reaped(run), &account, &opts, 3);
        let account = Readiness {
            username: username.into(),
            ..account
        };
        machine.advance(
            Event::ConnectionRestored(Binding {
                connection: ConnectionEpoch(2),
                ..binding()
            }),
            &account,
            &opts,
            4,
        );
        assert_eq!(machine.status().waiting_to_restart, restart);
        assert_eq!(machine.status().active.is_some(), restart);
    }
}

#[test]
fn readiness_polls_until_the_third_read_starts_the_run() {
    let mut machine = Lifecycle::new(binding());
    send(&mut machine, Event::Command(Command::Start(request())), 0);
    let run = machine.status().active.unwrap().run;
    for at in [1, 102] {
        let reply = send(&mut machine, Event::PlayerReady { run, ready: false }, at);
        assert!(reply.effects.contains(&Effect::ScheduleDeadline(at + 100)));
        assert!(terminals(&reply).is_empty());
        assert!(
            send(&mut machine, Event::Deadline, at + 99)
                .effects
                .is_empty()
        );
        let poll = send(&mut machine, Event::Deadline, at + 100);
        assert!(poll.effects.contains(&Effect::ReadPlayerReady {
            run,
            binding: binding(),
            deadline: 10_000,
        }));
        assert!(
            send(&mut machine, Event::ProjectionChanged, at + 100)
                .effects
                .is_empty()
        );
    }
    let ready = send(&mut machine, Event::PlayerReady { run, ready: true }, 203);
    assert!(
        ready
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Spawn { run: id, .. } if *id == run))
    );
    assert_eq!(machine.status().active.unwrap().phase, RunPhase::Booting);
}

#[test]
fn readiness_polling_never_extends_the_ten_second_deadline() {
    let mut machine = Lifecycle::new(binding());
    send(&mut machine, Event::Command(Command::Start(request())), 0);
    let run = machine.status().active.unwrap().run;
    for at in (0..10_000).step_by(100) {
        if at > 0 {
            let poll = send(&mut machine, Event::Deadline, at);
            assert!(poll.effects.contains(&Effect::ReadPlayerReady {
                run,
                binding: binding(),
                deadline: 10_000,
            }));
        }
        let reply = send(&mut machine, Event::PlayerReady { run, ready: false }, at);
        assert!(reply.effects.contains(&Effect::ScheduleDeadline(at + 100)));
        assert!(terminals(&reply).is_empty());
    }
    assert!(send(&mut machine, Event::Deadline, 9999).effects.is_empty());
    let expired = send(&mut machine, Event::Deadline, 10_000);
    assert_eq!(
        terminals(&expired),
        vec![Outcome::Failed(
            "Script readiness timed out after 10 s".into()
        )]
    );
    assert!(
        !expired
            .effects
            .iter()
            .any(|e| matches!(e, Effect::ReadPlayerReady { .. } | Effect::Spawn { .. }))
    );
    assert!(
        send(
            &mut machine,
            Event::PlayerReady { run, ready: true },
            10_001
        )
        .effects
        .is_empty()
    );
}

#[test]
fn readiness_polling_waits_for_lost_facts_without_accepting_an_old_answer() {
    for missing in [
        Readiness {
            logged_in: false,
            ..readiness()
        },
        Readiness {
            username: " ".into(),
            ..readiness()
        },
        Readiness {
            projections_ready: false,
            ..readiness()
        },
    ] {
        let mut machine = Lifecycle::new(binding());
        send(&mut machine, Event::Command(Command::Start(request())), 0);
        let run = machine.status().active.unwrap().run;
        machine.advance(Event::ProjectionChanged, &missing, &options(), 1);
        let rejected = machine.advance(
            Event::PlayerReady { run, ready: true },
            &missing,
            &options(),
            2,
        );
        assert!(
            !rejected
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Spawn { .. }))
        );
        assert!(
            machine
                .advance(Event::Deadline, &missing, &options(), 102)
                .effects
                .is_empty()
        );
        let resumed = send(&mut machine, Event::ProjectionChanged, 103);
        assert!(resumed.effects.contains(&Effect::ReadPlayerReady {
            run,
            binding: binding(),
            deadline: 10_000,
        }));
        let ready = send(&mut machine, Event::PlayerReady { run, ready: true }, 104);
        assert!(
            ready
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Spawn { .. }))
        );
    }
}

#[test]
fn stop_cancels_only_the_matching_pending_restart() {
    for reaped in [false, true] {
        let mut machine = Lifecycle::new(binding());
        let run = start(&mut machine);
        let opts = Options {
            restart_after_reconnect: true,
            ..options()
        };
        machine.advance(Event::ConnectionLost(binding()), &readiness(), &opts, 10);
        if reaped {
            machine.advance(Event::Reaped(run), &readiness(), &opts, 11);
        }
        machine.advance(
            Event::Command(Command::Stop {
                run: RunId(run.0 + 1),
                reason: String::new(),
            }),
            &readiness(),
            &opts,
            12,
        );
        assert!(machine.status().waiting_to_restart);
        machine.advance(
            Event::Command(Command::Stop {
                run,
                reason: String::new(),
            }),
            &readiness(),
            &opts,
            13,
        );
        assert!(!machine.status().waiting_to_restart);
        machine.advance(Event::Reaped(run), &readiness(), &opts, 14);
        let result = machine.advance(
            Event::ConnectionRestored(binding()),
            &readiness(),
            &opts,
            15,
        );
        assert!(machine.status().active.is_none());
        assert!(terminals(&result).is_empty());
    }
}
