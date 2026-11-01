use super::types::*;

const CAPACITY: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuePhase {
    Idle,
    Preparing,
    Running,
    Paused,
    Stopping,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueRunStatus {
    Running,
    Paused,
    Completed,
    Failed,
    Stopped,
    Exited,
    Canceled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemState {
    Pending,
    Active,
    Finished(Outcome),
}
#[derive(Clone, Debug, PartialEq)]
pub struct QueueItem {
    pub entry: Entry,
    pub state: ItemState,
}
#[derive(Clone, Debug, PartialEq)]
pub struct QueueRun {
    pub status: QueueRunStatus,
    pub items: Vec<QueueItem>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct QueueStatus {
    pub phase: QueuePhase,
    pub entries: Vec<Entry>,
    pub current: Option<usize>,
    pub latest: Option<QueueRun>,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub enum Command {
    Add(Entry),
    Edit(Entry),
    Move {
        entry: EntryId,
        direction: Direction,
    },
    Remove(EntryId),
    Clear,
    Start,
    Next,
    Cancel(String),
}
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Up,
    Down,
}
#[derive(Clone, Debug)]
pub(super) enum Action {
    Prepare { id: PreparationId, entry: Entry },
    Launch(Entry),
    Stop(String),
}
#[derive(Debug)]
pub(super) struct Queue {
    status: QueueStatus,
    preparation: Option<(PreparationId, usize)>,
    sequence: u64,
}
impl Default for Queue {
    fn default() -> Self {
        Self {
            status: QueueStatus {
                phase: QueuePhase::Idle,
                entries: vec![],
                current: None,
                latest: None,
                error: None,
            },
            preparation: None,
            sequence: 0,
        }
    }
}
impl Queue {
    pub fn status(&self) -> QueueStatus {
        self.status.clone()
    }
    pub fn command(&mut self, command: Command) -> Result<Vec<Action>, Rejection> {
        match command {
            Command::Cancel(reason) => match self.status.phase {
                QueuePhase::Idle | QueuePhase::Stopping => {}
                QueuePhase::Preparing => {
                    self.preparation = None;
                    self.status.phase = QueuePhase::Idle;
                }
                QueuePhase::Paused => self.finish(QueueRunStatus::Canceled),
                QueuePhase::Running => {
                    self.status.phase = QueuePhase::Stopping;
                    self.status.latest.as_mut().unwrap().status = QueueRunStatus::Canceled;
                    return Ok(vec![Action::Stop(reason)]);
                }
            },
            Command::Next => {
                if self.status.phase == QueuePhase::Paused {
                    return Ok(self.launch(self.status.current.unwrap() + 1));
                }
            }
            command => {
                if self.status.phase != QueuePhase::Idle {
                    return Err(Rejection::Busy);
                }
                match command {
                    Command::Add(entry) => {
                        if self.status.entries.len() == CAPACITY {
                            return Err(Rejection::Full);
                        }
                        if self.status.entries.iter().any(|e| e.id == entry.id) {
                            return Err(Rejection::DuplicateEntry);
                        }
                        self.status.entries.push(entry);
                        self.status.error = None;
                    }
                    Command::Edit(entry) => {
                        if let Some(current) =
                            self.status.entries.iter_mut().find(|e| e.id == entry.id)
                        {
                            *current = entry;
                            self.status.error = None;
                        }
                    }
                    Command::Move { entry, direction } => {
                        if let Some(index) = self.status.entries.iter().position(|e| e.id == entry)
                        {
                            let next = match direction {
                                Direction::Up => index.checked_sub(1),
                                Direction::Down => index.checked_add(1),
                            };
                            if let Some(next) = next.filter(|i| *i < self.status.entries.len()) {
                                self.status.entries.swap(index, next);
                            }
                        }
                    }
                    Command::Remove(id) => {
                        self.status.entries.retain(|e| e.id != id);
                    }
                    Command::Clear => {
                        self.status.entries.clear();
                        self.status.latest = None;
                        self.status.current = None;
                        self.status.error = None;
                    }
                    Command::Start => {
                        if self.status.entries.is_empty() {
                            self.status.error =
                                Some("Add at least one script before starting the queue.".into());
                            return Err(Rejection::Empty);
                        }
                        self.status.error = None;
                        let action = self.prepare(0)?;
                        self.status.phase = QueuePhase::Preparing;
                        return Ok(vec![action]);
                    }
                    Command::Next | Command::Cancel(_) => unreachable!(),
                }
            }
        }
        Ok(vec![])
    }
    fn prepare(&mut self, index: usize) -> Result<Action, Rejection> {
        self.sequence = self.sequence.checked_add(1).ok_or(Rejection::IdExhausted)?;
        let id = PreparationId(self.sequence);
        self.preparation = Some((id, index));
        Ok(Action::Prepare {
            id,
            entry: self.status.entries[index].clone(),
        })
    }
    pub fn prepared(&mut self, id: PreparationId, result: Result<Entry, String>) -> Vec<Action> {
        let Some((pending, index)) = self.preparation else {
            return vec![];
        };
        if pending != id {
            return vec![];
        }
        self.preparation = None;
        match result {
            Ok(entry) if entry.id == self.status.entries[index].id => {
                self.status.entries[index] = entry
            }
            result => {
                self.status.phase = QueuePhase::Idle;
                self.status.error = Some(
                    result
                        .err()
                        .unwrap_or_else(|| "Queue entry identity changed".into()),
                );
                return vec![];
            }
        }
        if index + 1 < self.status.entries.len() {
            match self.prepare(index + 1) {
                Ok(action) => return vec![action],
                Err(_) => {
                    self.status.phase = QueuePhase::Idle;
                    self.status.error = Some("Queue identity exhausted".into());
                    return vec![];
                }
            }
        }
        self.status.latest = Some(QueueRun {
            status: QueueRunStatus::Running,
            items: self
                .status
                .entries
                .iter()
                .cloned()
                .map(|entry| QueueItem {
                    entry,
                    state: ItemState::Pending,
                })
                .collect(),
        });
        self.launch(0)
    }
    fn launch(&mut self, index: usize) -> Vec<Action> {
        let run = self.status.latest.as_mut().unwrap();
        let item = &mut run.items[index];
        item.state = ItemState::Active;
        run.status = QueueRunStatus::Running;
        self.status.phase = QueuePhase::Running;
        self.status.current = Some(index);
        vec![Action::Launch(item.entry.clone())]
    }
    fn finish(&mut self, status: QueueRunStatus) {
        self.status.latest.as_mut().unwrap().status = status;
        self.status.phase = QueuePhase::Idle;
        self.status.current = None;
    }
    pub fn terminal(&mut self, outcome: Outcome) -> Vec<Action> {
        if !matches!(
            self.status.phase,
            QueuePhase::Running | QueuePhase::Stopping
        ) {
            return vec![];
        }
        let index = self.status.current.unwrap();
        let run = self.status.latest.as_mut().unwrap();
        run.items[index].state = ItemState::Finished(outcome.clone());
        let has_next = index + 1 < run.items.len();
        if self.status.phase == QueuePhase::Stopping {
            self.finish(QueueRunStatus::Canceled);
            return vec![];
        }
        match outcome {
            Outcome::Completed | Outcome::Stopped(StopReason::Script(_)) => {
                if has_next {
                    return self.launch(index + 1);
                }
                self.finish(QueueRunStatus::Completed);
            }
            Outcome::Failed(_) | Outcome::Killed(_) => {
                if has_next {
                    self.status.phase = QueuePhase::Paused;
                    run.status = QueueRunStatus::Paused;
                } else {
                    self.finish(QueueRunStatus::Failed);
                }
            }
            Outcome::Stopped(_) => self.finish(QueueRunStatus::Stopped),
            Outcome::Exited { .. } => self.finish(QueueRunStatus::Exited),
        }
        vec![]
    }
}
