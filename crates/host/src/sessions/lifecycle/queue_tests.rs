use super::{queue::*, *};
use serde_json::{Value, json};

fn entry(name: &str, available: bool) -> Entry {
    Entry {
        id: EntryId(name.bytes().next().unwrap() as u64),
        request: Request {
            name: name.into(),
            reference: format!("{name}.js"),
            ..super::tests::request()
        },
        inputs_available: available,
    }
}
fn kind(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Completed => "completed",
        Outcome::Failed(_) | Outcome::Killed(_) => "failed",
        Outcome::Stopped(StopReason::Script(_)) => "script-stopped",
        Outcome::Stopped(_) => "externally-stopped",
        Outcome::Exited { .. } => "script-exited",
    }
}
fn outcome(kind: &str) -> Outcome {
    match kind {
        "completed" => Outcome::Completed,
        "failed" => Outcome::Failed("boom".into()),
        "script-stopped" => Outcome::Stopped(StopReason::Script(None)),
        "externally-stopped" => Outcome::Stopped(StopReason::Requested("user".into())),
        "script-exited" => Outcome::Exited {
            logout: false,
            close_client: false,
        },
        _ => panic!("unknown outcome"),
    }
}
#[test]
fn shared_queue_transcripts() {
    let scenarios: Vec<Value> = serde_json::from_str(include_str!("queue-corpus.json")).unwrap();
    for scenario in &scenarios {
        let mut queue = Queue::default();
        let mut preparations: Vec<(PreparationId, Entry)> = vec![];
        let mut starts = vec![];
        for op in scenario["operations"].as_array().unwrap() {
            let name = op["name"].as_str().unwrap_or("a");
            let available = op["available"].as_bool().unwrap_or(false);
            let actions = match op["op"].as_str().unwrap() {
                "add" => queue.command(QueueCommand::Add(entry(name, available))),
                "edit" => queue.command(QueueCommand::Edit(entry(name, available))),
                "move" => queue.command(QueueCommand::Move {
                    entry: entry(name, false).id,
                    direction: if op["offset"] == -1 {
                        Direction::Up
                    } else {
                        Direction::Down
                    },
                }),
                "remove" => queue.command(QueueCommand::Remove(entry(name, false).id)),
                "clear" => queue.command(QueueCommand::Clear),
                "start" => queue.command(QueueCommand::Start),
                "next" => queue.command(QueueCommand::Next),
                "cancel" => queue.command(QueueCommand::Cancel("user".into())),
                "terminal" => Ok(queue.terminal(outcome(op["kind"].as_str().unwrap()))),
                "prepared" => {
                    let mut actions = vec![];
                    while let Some((id, mut entry)) = preparations.pop() {
                        entry.inputs_available = available;
                        let answer = if op["failure"] == true {
                            Err("resolution failed".into())
                        } else {
                            Ok(entry)
                        };
                        for action in queue.prepared(id, answer) {
                            match action {
                                Action::Prepare { id, entry } => preparations.push((id, entry)),
                                action => actions.push(action),
                            }
                        }
                    }
                    Ok(actions)
                }
                _ => panic!("unknown command"),
            }
            .unwrap_or_default();
            for action in actions {
                match action {
                    Action::Prepare { id, entry } => preparations.push((id, entry)),
                    Action::Launch(entry) => starts.push(entry.request.name),
                    Action::Stop(_) => {}
                }
            }
            let s = queue.status();
            let phase = match s.phase {
                QueuePhase::Idle => "idle",
                QueuePhase::Preparing => "preparing",
                QueuePhase::Running => "running",
                QueuePhase::Paused => "paused",
                QueuePhase::Stopping => "stopping",
            };
            let status = s.latest.as_ref().map(|run| match run.status {
                QueueRunStatus::Running => "running",
                QueueRunStatus::Paused => "paused",
                QueueRunStatus::Completed => "completed",
                QueueRunStatus::Failed => "failed",
                QueueRunStatus::Stopped => "stopped",
                QueueRunStatus::Exited => "exited",
                QueueRunStatus::Canceled => "canceled",
            });
            let items: Vec<_> = s
                .latest
                .iter()
                .flat_map(|run| &run.items)
                .map(|item| match &item.state {
                    ItemState::Pending => "pending",
                    ItemState::Active => "active",
                    ItemState::Finished(outcome) => kind(outcome),
                })
                .collect();
            assert_eq!(
                json!({ "phase": phase, "status": status, "current": s.current, "entries": s.entries.iter().map(|e| &e.request.name).collect::<Vec<_>>(), "available": s.entries.iter().map(|e| e.inputs_available).collect::<Vec<_>>(), "items": items, "starts": starts }),
                op["expect"],
                "{}: {}",
                scenario["name"],
                op["op"]
            );
        }
        println!("PASS {}", scenario["name"].as_str().unwrap());
    }
    println!("Rust queue: {} shared scenarios passed", scenarios.len());
}
