use super::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Scenario {
    name: String,
    initial: Value,
    policy: RoomPolicy,
    commands: Vec<Expected>,
    updates: Vec<Update>,
    finished_at: u64,
    outcome: String,
    #[serde(default)]
    cancel_at: Option<u64>,
}
#[derive(Deserialize)]
struct Expected {
    method: String,
    args: Vec<Value>,
    at: u64,
    reply: Value,
    patch: Value,
}
#[derive(Deserialize)]
struct Update {
    at: u64,
    patch: Value,
}
fn patch(value: &mut Value, update: &Value) {
    for (key, item) in update.as_object().unwrap() {
        if item.is_object() && value[key].is_object() {
            patch(&mut value[key], item);
        } else {
            value[key] = item.clone();
        }
    }
}
#[test]
fn shared_safe_move_transcripts() {
    let scenarios: Vec<Scenario> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/safe_move/transcripts.json"
    ))
    .unwrap();
    for mut scenario in scenarios {
        let mut job = SafeMove::new(scenario.policy).unwrap();
        let mut snapshot = scenario.initial;
        let mut now = 0;
        let mut event = Event::Tick;
        let mut commands = scenario.commands.iter();
        let mut updates = scenario.updates.iter().peekable();
        let mut finished = false;
        for _ in 0..10000 {
            while updates.peek().is_some_and(|u| u.at <= now) {
                patch(&mut snapshot, &updates.next().unwrap().patch);
            }
            if scenario.cancel_at == Some(now) {
                event = Event::Cancel;
                scenario.cancel_at = None;
            }
            let effects = job.advance(
                &serde_json::from_value(snapshot.clone()).unwrap(),
                event,
                now,
            );
            if let Some(outcome) = effects.outcome {
                assert_eq!(
                    format!("{outcome:?}"),
                    scenario.outcome,
                    "{}",
                    scenario.name
                );
                assert!(
                    commands.next().is_none(),
                    "{}: missing commands",
                    scenario.name
                );
                assert_eq!(now, scenario.finished_at, "{}", scenario.name);
                finished = true;
                break;
            }
            if let Some(command) = effects.command {
                let expected = commands
                    .next()
                    .unwrap_or_else(|| panic!("{}: unexpected {command:?}", scenario.name));
                assert_eq!(
                    command.action.loader_call(),
                    (expected.method.as_str(), expected.args.clone()),
                    "{}",
                    scenario.name
                );
                assert_eq!(now, expected.at, "{}", scenario.name);
                patch(&mut snapshot, &expected.patch);
                let reply = if let Some(error) = expected.reply.get("error").and_then(Value::as_str)
                {
                    Reply::Error(error.into())
                } else {
                    match command.action {
                        Action::Travel(travel::Action::CellReady(_)) => Reply::Travel(
                            travel::Reply::CellReady(expected.reply.as_bool().unwrap()),
                        ),
                        Action::Travel(_) => Reply::Travel(if expected.reply == json!(true) {
                            travel::Reply::Accepted
                        } else {
                            travel::Reply::Unavailable
                        }),
                        Action::House(_) | Action::SetReturnInfo(_) => {
                            if expected.reply == json!(true) {
                                Reply::Accepted
                            } else {
                                Reply::Unavailable
                            }
                        }
                        _ if expected.reply == json!({"unavailable":true}) => Reply::Unavailable,
                        _ => Reply::Value(expected.reply.clone()),
                    }
                };
                event = Event::Completed {
                    id: command.id,
                    reply,
                };
            } else {
                now = effects.next_deadline.expect("stalled");
                event = Event::Tick;
            }
        }
        assert!(finished, "{}: unbounded", scenario.name);
        println!("{} passed at {}ms", scenario.name, now);
    }
}

fn house_snapshot() -> Snapshot {
    let scenarios: Vec<Scenario> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/safe_move/transcripts.json"
    ))
    .unwrap();
    serde_json::from_value(
        scenarios
            .into_iter()
            .find(|s| s.name == "another house moves to own house")
            .unwrap()
            .initial,
    )
    .unwrap()
}

#[test]
fn restores_return_info_before_reporting_a_move_error() {
    let snapshot = house_snapshot();
    let mut job = SafeMove::new(RoomPolicy::Public).unwrap();
    assert_eq!(
        job.advance(&snapshot, Event::Tick, 0).next_deadline,
        Some(1000)
    );
    let command = job.advance(&snapshot, Event::Tick, 1000).command.unwrap();
    assert!(
        job.advance(
            &snapshot,
            Event::Completed {
                id: command.id + 100,
                reply: Reply::Value(json!(true))
            },
            1000
        )
        .command
        .is_none()
    );
    let command = job
        .advance(
            &snapshot,
            Event::Completed {
                id: command.id,
                reply: Reply::Value(json!(false)),
            },
            1000,
        )
        .command
        .unwrap();
    let saved = json!({"map":"old", "nested":[null, 42]});
    let command = job
        .advance(
            &snapshot,
            Event::Completed {
                id: command.id,
                reply: Reply::Value(saved.clone()),
            },
            1000,
        )
        .command
        .unwrap();
    assert_eq!(command.action, Action::SetReturnInfo(Value::Null));
    let command = job
        .advance(
            &snapshot,
            Event::Completed {
                id: command.id,
                reply: Reply::Accepted,
            },
            1000,
        )
        .command
        .unwrap();
    let effects = job.advance(
        &snapshot,
        Event::Completed {
            id: command.id,
            reply: Reply::Error("move failed".into()),
        },
        1000,
    );
    assert!(effects.outcome.is_none());
    let command = effects.command.unwrap();
    assert_eq!(command.action, Action::SetReturnInfo(saved));
    let effects = job.advance(
        &snapshot,
        Event::Completed {
            id: command.id,
            reply: Reply::Accepted,
        },
        1000,
    );
    assert_eq!(effects.outcome, Some(Outcome::Error("move failed".into())));
    assert!(job.advance(&snapshot, Event::Tick, 2000).outcome.is_none());
}

#[test]
fn validates_room_inputs_and_bounds_saved_data() {
    assert!(SafeMove::new(RoomPolicy::Specific { room: 0 }).is_err());
    assert!(SafeMove::new(RoomPolicy::RandomPrivate { room: 9999 }).is_err());
    let mut snapshot = house_snapshot();
    snapshot.username = "x".repeat(4097);
    let mut job = SafeMove::new(RoomPolicy::Public).unwrap();
    assert!(matches!(
        job.advance(&snapshot, Event::Tick, 0).outcome,
        Some(Outcome::Error(_))
    ));
    snapshot.username = "test".into();
    let mut job = SafeMove::new(RoomPolicy::Public).unwrap();
    job.advance(&snapshot, Event::Tick, 0);
    let command = job.advance(&snapshot, Event::Tick, 1000).command.unwrap();
    let command = job
        .advance(
            &snapshot,
            Event::Completed {
                id: command.id,
                reply: Reply::Value(json!(false)),
            },
            1000,
        )
        .command
        .unwrap();
    assert_eq!(
        job.advance(
            &snapshot,
            Event::Completed {
                id: command.id,
                reply: Reply::Value(json!("x".repeat(65536)))
            },
            1000
        )
        .outcome,
        Some(Outcome::Unavailable)
    );
}
