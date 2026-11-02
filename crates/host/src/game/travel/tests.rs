use super::*;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Scenario {
    name: String,
    request: Request,
    initial: Value,
    commands: Vec<Expected>,
    outcome: bool,
    finished_at: u64,
    updates: Vec<Update>,
}
#[derive(Deserialize)]
struct Expected {
    method: String,
    at: u64,
    args: Vec<String>,
    patch: Value,
    reply: bool,
    #[serde(default)]
    delay_ms: u64,
}
#[derive(Deserialize)]
struct Update {
    at: u64,
    patch: Value,
}
fn patch(snapshot: &mut Value, patch: &Value) {
    for (key, value) in patch.as_object().unwrap() {
        snapshot[key] = value.clone();
    }
}
#[test]
fn shared_travel_transcripts() {
    replay(include_str!(
        "../../../tests/fixtures/travel/transcripts.json"
    ));
}

#[test]
fn ascii_identifiers_deliberately_diverge_from_ts_for_non_game_unicode() {
    replay(include_str!(
        "../../../tests/fixtures/travel/non_game_unicode_divergence.json"
    ));
}

fn replay(corpus: &str) {
    let scenarios: Vec<Scenario> = serde_json::from_str(corpus).unwrap();
    for scenario in scenarios {
        let mut job = TravelJob::new(scenario.request).unwrap();
        let mut snapshot = scenario.initial;
        let mut now = 0;
        let mut event = Event::Tick;
        let mut commands = scenario.commands.iter();
        let mut updates = scenario.updates.iter().peekable();
        let mut completed = false;
        for _ in 0..5000 {
            while updates.peek().is_some_and(|update| update.at <= now) {
                patch(&mut snapshot, &updates.next().unwrap().patch);
            }
            let effects = job.advance(
                &serde_json::from_value(snapshot.clone()).unwrap(),
                event,
                now,
            );
            if let Some(outcome) = effects.outcome {
                assert_eq!(
                    outcome,
                    Outcome::Returned(scenario.outcome),
                    "{}",
                    scenario.name
                );
                assert!(
                    commands.next().is_none(),
                    "{}: unissued commands",
                    scenario.name
                );
                assert_eq!(
                    now, scenario.finished_at,
                    "{}: completion clock",
                    scenario.name
                );
                completed = true;
                break;
            }
            if let Some(command) = effects.command {
                let expected = commands
                    .next()
                    .unwrap_or_else(|| panic!("{}: unexpected {command:?}", scenario.name));
                assert_eq!(
                    command.action.loader_call(),
                    (expected.method.as_str(), expected.args.clone()),
                    "{} at {now}",
                    scenario.name
                );
                assert_eq!(now, expected.at, "{}: command clock", scenario.name);
                patch(&mut snapshot, &expected.patch);
                let reply_at = now + expected.delay_ms;
                if effects
                    .next_deadline
                    .is_some_and(|deadline| deadline < reply_at)
                {
                    now = effects.next_deadline.unwrap();
                    event = Event::Tick;
                    continue;
                }
                now = reply_at;
                event = Event::Completed {
                    id: command.id,
                    reply: if matches!(command.action, Action::CellReady(_)) {
                        Reply::CellReady(expected.reply)
                    } else if expected.reply {
                        Reply::Accepted
                    } else {
                        Reply::Unavailable
                    },
                };
            } else {
                now = effects
                    .next_deadline
                    .unwrap_or_else(|| panic!("{}: stalled", scenario.name));
                event = Event::Tick;
            }
        }
        assert!(completed, "{}: unbounded", scenario.name);
    }
}

#[test]
fn cancellation_ignores_late_replies() {
    let mut job = TravelJob::new(Request::Jump {
        cell: "Safe".into(),
        pad: None,
    })
    .unwrap();
    let scenarios: Vec<Scenario> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/travel/transcripts.json"
    ))
    .unwrap();
    let snapshot: Snapshot = serde_json::from_value(scenarios[0].initial.clone()).unwrap();
    let command = job.advance(&snapshot, Event::Tick, 0).command.unwrap();
    assert!(
        job.advance(
            &snapshot,
            Event::Completed {
                id: command.id + 1,
                reply: Reply::Accepted
            },
            0
        )
        .command
        .is_none()
    );
    assert_eq!(
        job.advance(&snapshot, Event::Cancel, 0).outcome,
        Some(Outcome::Cancelled)
    );
    let late = job.advance(
        &snapshot,
        Event::Completed {
            id: command.id,
            reply: Reply::Accepted,
        },
        1,
    );
    assert!(late.command.is_none());
    assert!(late.outcome.is_none());
}

#[test]
fn rejects_unbounded_input_and_requires_a_valid_random_draw() {
    assert_eq!(
        TravelJob::new(Request::Jump {
            cell: "x".repeat(MAX_TEXT + 1),
            pad: None
        })
        .unwrap_err(),
        InputError::TooLarge
    );
    assert_eq!(
        TravelJob::new(Request::Join {
            target: "doom-private".into(),
            cell: None,
            pad: None,
            random_room: None
        })
        .unwrap_err(),
        InputError::RandomRoomRequired
    );
}

#[test]
fn cell_readiness_cannot_outlive_the_jump_deadline() {
    let mut job = TravelJob::new(Request::Jump {
        cell: "r2".into(),
        pad: None,
    })
    .unwrap();
    let mut snapshot: Snapshot = serde_json::from_value(
        serde_json::from_str::<Vec<Scenario>>(include_str!(
            "../../../tests/fixtures/travel/transcripts.json"
        ))
        .unwrap()[0]
            .initial
            .clone(),
    )
    .unwrap();
    snapshot.live_cell = "r2".into();
    let command = job.advance(&snapshot, Event::Tick, 0).command.unwrap();
    let effects = job.advance(
        &snapshot,
        Event::Completed {
            id: command.id,
            reply: Reply::Accepted,
        },
        0,
    );
    let effects = if effects.command.is_none() {
        job.advance(&snapshot, Event::Tick, 0)
    } else {
        effects
    };
    assert!(matches!(
        effects.command.unwrap().action,
        Action::CellReady(_)
    ));
    assert_eq!(effects.next_deadline, Some(3000));
    assert_eq!(
        job.advance(&snapshot, Event::Tick, 3000).outcome,
        Some(Outcome::Returned(false))
    );
}

#[test]
fn public_rooms_are_reused_and_private_or_invalid_rooms_join_exactly() {
    for (target, joined) in [
        (" battleon ", None),
        ("battleon-1", None),
        ("battleon-999", None),
        ("battleon-1000", None),
        ("battleon-1001", Some(("battleon-1001", 1001))),
        ("battleon-99999", Some(("battleon-99999", 99999))),
        ("battleon-100000", None),
        ("battleon-private", Some(("battleon-10000", 10000))),
        ("battleon-0", Some(("battleon-10000", 10000))),
        ("battleon-1.5", Some(("battleon-10000", 10000))),
    ] {
        let mut job = TravelJob::new(Request::Join {
            target: target.into(),
            cell: None,
            pad: None,
            random_room: Some(10000),
        })
        .unwrap();
        let mut snapshot = Snapshot {
            map: "battleon".into(),
            room: 1,
            loaded: true,
            ready: true,
            tfer: true,
            alive: true,
            ..Default::default()
        };
        let mut event = Event::Tick;
        let mut joins = Vec::new();
        let mut outcome = None;
        for _ in 0..20 {
            let effects = job.advance(&snapshot, event, 0);
            if let Some(value) = effects.outcome {
                outcome = Some(value);
                break;
            }
            if let Some(command) = effects.command {
                let (map, room) = joined.unwrap_or_else(|| panic!("unexpected join for {target}"));
                assert_eq!(
                    command.action.loader_call(),
                    ("player.joinMap", vec![map.into()])
                );
                joins.push(map.to_owned());
                snapshot.room = room;
                event = Event::Completed {
                    id: command.id,
                    reply: Reply::Accepted,
                };
            } else {
                assert_eq!(effects.next_deadline, Some(0), "{target}");
                event = Event::Tick;
            }
        }
        assert_eq!(outcome, Some(Outcome::Returned(true)), "{target}");
        assert_eq!(
            joins,
            joined
                .map(|(map, _)| map.to_owned())
                .into_iter()
                .collect::<Vec<_>>(),
            "{target}"
        );
    }
}
