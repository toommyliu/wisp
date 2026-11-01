mod target;
#[cfg(test)]
mod tests;
mod text;

use text::same_text;

mod types;
pub use types::*;

#[derive(Clone, Debug)]
struct Destination {
    map: String,
    name: String,
    exact_room: Option<u64>,
}
#[derive(Clone, Copy, Debug)]
enum Wait {
    ReadyBefore,
    ReadyAfter,
    Tfer,
    Alive,
    Load,
}
#[derive(Clone, Copy, Debug)]
enum Resume {
    Return,
    Join,
    Exit,
}
#[derive(Clone, Copy, Debug)]
enum Observation {
    AwaitingRequest,
    Requested,
}
#[derive(Clone, Copy, Debug)]
enum Location {
    Requested,
    Redirected,
}
#[derive(Clone, Debug)]
struct Jump {
    cell: String,
    pad: Option<String>,
    observation: Observation,
    expires: u64,
    resume: Resume,
}
#[derive(Clone, Debug)]
enum Pending {
    Join,
    Jump(Jump),
    CellReady { jump: Jump, location: Location },
    Stop,
    ClearTarget,
}
#[derive(Clone, Debug)]
enum Phase {
    New,
    Wait { kind: Wait, expires: u64, next: u64 },
    Pending { id: u64, action: Pending },
    Jump { jump: Jump, next: u64 },
    StartExit(Resume),
    ExitNext,
    Idle { expires: u64, next: u64 },
    ConfirmIdle { expires: u64, at: u64 },
    Done,
}
#[derive(Clone, Debug)]
pub struct TravelJob {
    request: Request,
    destination: Option<Destination>,
    phase: Phase,
    sequence: u64,
    exit_cells: Vec<String>,
    exit_cell: String,
    exit_pad: String,
    exit_index: usize,
    exit_resume: Resume,
}
const MAX_TEXT: usize = 4096;
const MAX_CELLS: usize = 2048;
const MAX_MONSTERS: usize = 2048;

fn args(cell: &str, pad: &Option<String>) -> Vec<String> {
    let mut result = vec![cell.to_owned()];
    result.extend(pad.iter().cloned());
    result
}
impl TravelJob {
    pub fn new(request: Request) -> Result<Self, InputError> {
        let fields = match &request {
            Request::Join {
                target, cell, pad, ..
            } => vec![Some(target), cell.as_ref(), pad.as_ref()],
            Request::Jump { cell, pad } => vec![Some(cell), pad.as_ref()],
            Request::Exit => vec![],
        };
        if fields.iter().flatten().any(|text| text.len() > MAX_TEXT) {
            return Err(InputError::TooLarge);
        }
        let destination = target::parse(&request)?;
        Ok(Self {
            request,
            destination,
            phase: Phase::New,
            sequence: 0,
            exit_cells: vec![],
            exit_cell: String::new(),
            exit_pad: String::new(),
            exit_index: 0,
            exit_resume: Resume::Return,
        })
    }
    pub fn reads(&self) -> Reads {
        match &self.phase {
            Phase::New => match self.request {
                Request::Join { .. } => Reads::LOAD,
                Request::Jump { .. } => Reads::default(),
                Request::Exit => Reads::STATE,
            },
            Phase::Wait { kind, .. } => match kind {
                Wait::ReadyBefore | Wait::ReadyAfter => Reads::READY,
                Wait::Tfer => Reads::TFER,
                Wait::Alive => Reads::ALIVE.with(Reads::STATE),
                Wait::Load => Reads::LOAD,
            },
            Phase::Jump { .. } => Reads::LIVE,
            Phase::StartExit(_) => Reads::EXIT,
            Phase::Idle { .. } | Phase::ConfirmIdle { .. } => Reads::STATE,
            _ => Reads::default(),
        }
    }
    pub fn observation_deadline(&self) -> Option<u64> {
        match &self.phase {
            Phase::Wait { expires, .. }
            | Phase::Idle { expires, .. }
            | Phase::ConfirmIdle { expires, .. } => Some(*expires),
            Phase::Jump { jump, .. } => Some(jump.expires),
            _ => None,
        }
    }
    fn finish(&mut self, value: bool) -> Effects {
        self.phase = Phase::Done;
        Effects {
            outcome: Some(Outcome::Returned(value)),
            ..Default::default()
        }
    }
    fn issue(&mut self, action: Action, pending: Pending) -> Effects {
        self.sequence += 1;
        let next_deadline = match &pending {
            Pending::CellReady { jump, .. } => Some(jump.expires),
            _ => None,
        };
        self.phase = Phase::Pending {
            id: self.sequence,
            action: pending,
        };
        Effects {
            command: Some(Command {
                id: self.sequence,
                action,
            }),
            next_deadline,
            ..Default::default()
        }
    }
    fn wait(&mut self, kind: Wait, duration: u64, now: u64) {
        self.phase = Phase::Wait {
            kind,
            expires: now.saturating_add(duration),
            next: now,
        };
    }
    fn loaded(&self, snapshot: &Snapshot) -> bool {
        let destination = self.destination.as_ref().unwrap();
        snapshot.loaded
            && same_text(&snapshot.map, &destination.name)
            && destination
                .exact_room
                .is_none_or(|room| room == snapshot.room)
    }
    fn start_jump(
        &mut self,
        cell: String,
        pad: Option<String>,
        resume: Resume,
        now: u64,
    ) -> Effects {
        let cell = text::trim(&cell).to_owned();
        if cell.is_empty() {
            return self.resume_jump(false, resume, now);
        }
        let jump = Jump {
            cell,
            pad,
            observation: Observation::AwaitingRequest,
            expires: 0,
            resume,
        };
        self.issue(
            Action::Jump(args(&jump.cell, &jump.pad)),
            Pending::Jump(jump),
        )
    }
    fn resume_jump(&mut self, value: bool, resume: Resume, now: u64) -> Effects {
        match resume {
            Resume::Return | Resume::Join => self.finish(value),
            Resume::Exit => {
                self.phase = Phase::Idle {
                    expires: now.saturating_add(2000),
                    next: now,
                };
                Effects {
                    next_deadline: Some(now),
                    ..Default::default()
                }
            }
        }
    }
    fn after_ready(&mut self, now: u64) -> Effects {
        let Request::Join { cell, pad, .. } = &self.request else {
            unreachable!()
        };
        match cell
            .clone()
            .or_else(|| pad.as_ref().map(|_| "Enter".to_owned()))
        {
            Some(cell) => self.start_jump(cell, pad.clone(), Resume::Return, now),
            None => self.finish(true),
        }
    }
    fn start_exit(&mut self, snapshot: &Snapshot, resume: Resume) {
        self.exit_resume = resume;
        self.exit_cell = if snapshot.cell.is_empty() && snapshot.loaded {
            snapshot.live_cell.clone()
        } else {
            snapshot.cell.clone()
        };
        self.exit_pad = if snapshot.pad.is_empty() && snapshot.loaded {
            snapshot.live_pad.clone()
        } else {
            snapshot.pad.clone()
        };
        let key = |cell: &str| text::trim(cell).to_lowercase();
        let occupied: std::collections::BTreeSet<_> = snapshot
            .monsters
            .iter()
            .map(|monster| key(&monster.cell))
            .collect();
        let aggressive: std::collections::BTreeSet<_> = snapshot
            .monsters
            .iter()
            .filter(|monster| monster.aggressive)
            .map(|monster| key(&monster.cell))
            .collect();
        let current = key(&self.exit_cell);
        self.exit_cells = snapshot
            .cells
            .iter()
            .filter(|cell| {
                let cell = key(cell);
                !["", "blank", "wait"].contains(&cell.as_str()) && !aggressive.contains(&cell)
            })
            .cloned()
            .collect();
        self.exit_cells.sort_by_key(|cell| {
            let cell = key(cell);
            if cell == current {
                0
            } else if occupied.contains(&cell) {
                2
            } else {
                1
            }
        });
        self.exit_cells.truncate(3);
        self.phase = Phase::ExitNext;
    }
    fn after_exit(&mut self, value: bool) -> Effects {
        if !value || matches!(self.exit_resume, Resume::Return) {
            return self.finish(value);
        }
        let Request::Join { cell, pad, .. } = &self.request else {
            unreachable!()
        };
        let mut parameters = vec![self.destination.as_ref().unwrap().map.clone()];
        if cell.is_some() || pad.is_some() {
            parameters.extend(args(cell.as_deref().unwrap_or("Enter"), pad));
        }
        self.issue(Action::Join(parameters), Pending::Join)
    }
    pub fn advance(&mut self, snapshot: &Snapshot, event: Event, now: u64) -> Effects {
        if matches!(self.phase, Phase::Done) {
            return Effects::default();
        }
        if matches!(event, Event::Cancel) {
            self.phase = Phase::Done;
            return Effects {
                outcome: Some(Outcome::Cancelled),
                ..Default::default()
            };
        }
        if snapshot.cells.len() > MAX_CELLS
            || snapshot.monsters.len() > MAX_MONSTERS
            || [
                &snapshot.map,
                &snapshot.cell,
                &snapshot.pad,
                &snapshot.live_cell,
                &snapshot.live_pad,
            ]
            .iter()
            .any(|s| s.len() > MAX_TEXT)
            || snapshot.cells.iter().any(|s| s.len() > MAX_TEXT)
            || snapshot.monsters.iter().any(|m| m.cell.len() > MAX_TEXT)
        {
            return self.finish(false);
        }
        if let Phase::Pending {
            action: Pending::CellReady { jump, .. },
            ..
        } = &self.phase
            && now >= jump.expires
        {
            return self.resume_jump(false, jump.resume, now);
        }
        let reads = self.reads();
        if let Event::Completed { id, reply } = event {
            let Phase::Pending {
                id: expected,
                action,
            } = self.phase.clone()
            else {
                return Effects::default();
            };
            if id != expected {
                return Effects::default();
            }
            match action {
                Pending::Join => {
                    if reply != Reply::Accepted {
                        return self.finish(false);
                    }
                    self.wait(Wait::Load, 20000, now);
                }
                Pending::Jump(mut jump) => {
                    if reply != Reply::Accepted {
                        return self.resume_jump(false, jump.resume, now);
                    }
                    jump.expires = now.saturating_add(3000);
                    self.phase = Phase::Jump { jump, next: now };
                }
                Pending::CellReady { jump, location } => {
                    if reply == Reply::CellReady(true) {
                        return self.resume_jump(
                            matches!(location, Location::Requested),
                            jump.resume,
                            now,
                        );
                    }
                    let next = now.saturating_add(100).min(jump.expires);
                    self.phase = Phase::Jump { jump, next };
                    return Effects {
                        next_deadline: Some(next),
                        ..Default::default()
                    };
                }
                Pending::Stop => return self.issue(Action::CancelTarget, Pending::ClearTarget),
                Pending::ClearTarget => {
                    let cell = self.exit_cells[self.exit_index].clone();
                    self.exit_index += 1;
                    let pad = same_text(&cell, &self.exit_cell).then(|| self.exit_pad.clone());
                    return self.start_jump(cell, pad, Resume::Exit, now);
                }
            }
        }
        for _ in 0..8 {
            if self.reads() != reads {
                break;
            }
            match self.phase.clone() {
                Phase::New => match self.request.clone() {
                    Request::Join { .. } => {
                        if self.destination.as_ref().unwrap().map.is_empty() {
                            return self.finish(false);
                        }
                        self.wait(
                            if self.loaded(snapshot) {
                                Wait::ReadyAfter
                            } else {
                                Wait::ReadyBefore
                            },
                            10000,
                            now,
                        );
                    }
                    Request::Jump { cell, pad } => {
                        return self.start_jump(cell, pad, Resume::Return, now);
                    }
                    Request::Exit => {
                        if snapshot.state == CombatState::Idle {
                            return self.finish(true);
                        }
                        self.phase = Phase::StartExit(Resume::Return);
                    }
                },
                Phase::Wait {
                    kind,
                    expires,
                    next,
                } => {
                    if now >= expires {
                        return self.finish(false);
                    }
                    if now < next {
                        return Effects {
                            next_deadline: Some(next),
                            ..Default::default()
                        };
                    }
                    let ready = match kind {
                        Wait::ReadyBefore | Wait::ReadyAfter => snapshot.ready,
                        Wait::Tfer => snapshot.tfer,
                        Wait::Alive => snapshot.alive,
                        Wait::Load => self.loaded(snapshot),
                    };
                    if !ready {
                        self.phase = Phase::Wait {
                            kind,
                            expires,
                            next: now.saturating_add(100).min(expires),
                        };
                        continue;
                    }
                    match kind {
                        Wait::ReadyBefore => self.wait(Wait::Tfer, 10000, now),
                        Wait::Tfer => self.wait(Wait::Alive, 15000, now),
                        Wait::Alive => {
                            if snapshot.state == CombatState::Idle {
                                self.exit_resume = Resume::Join;
                                return self.after_exit(true);
                            }
                            self.phase = Phase::StartExit(Resume::Join);
                        }
                        Wait::Load => self.wait(Wait::ReadyAfter, 10000, now),
                        Wait::ReadyAfter => return self.after_ready(now),
                    }
                }
                Phase::Jump { mut jump, next } => {
                    if now >= jump.expires {
                        return self.resume_jump(false, jump.resume, now);
                    }
                    if now < next {
                        return Effects {
                            next_deadline: Some(next),
                            ..Default::default()
                        };
                    }
                    let requested = same_text(&snapshot.live_cell, &jump.cell)
                        && jump
                            .pad
                            .as_ref()
                            .is_none_or(|pad| same_text(&snapshot.live_pad, pad));
                    if requested {
                        jump.observation = Observation::Requested;
                    }
                    let cell = if requested {
                        jump.cell.clone()
                    } else {
                        text::trim(&snapshot.live_cell).to_owned()
                    };
                    if matches!(jump.observation, Observation::Requested) && !cell.is_empty() {
                        let pad = if requested {
                            jump.pad.clone()
                        } else {
                            let pad = text::trim(&snapshot.live_pad);
                            (!pad.is_empty()).then(|| pad.to_owned())
                        };
                        return self.issue(
                            Action::CellReady(args(&cell, &pad)),
                            Pending::CellReady {
                                jump,
                                location: if requested {
                                    Location::Requested
                                } else {
                                    Location::Redirected
                                },
                            },
                        );
                    }
                    let next = now.saturating_add(100).min(jump.expires);
                    self.phase = Phase::Jump { jump, next };
                }
                Phase::StartExit(resume) => self.start_exit(snapshot, resume),
                Phase::ExitNext => {
                    if self.exit_index >= self.exit_cells.len() {
                        return self.after_exit(false);
                    }
                    return self.issue(Action::CancelAutoAttack, Pending::Stop);
                }
                Phase::Idle { expires, next } => {
                    if now >= expires {
                        self.phase = Phase::ExitNext;
                        continue;
                    }
                    if now < next {
                        return Effects {
                            next_deadline: Some(next),
                            ..Default::default()
                        };
                    }
                    if snapshot.state == CombatState::Idle {
                        self.phase = Phase::ConfirmIdle {
                            expires,
                            at: now.saturating_add(500).min(expires),
                        };
                    } else {
                        self.phase = Phase::Idle {
                            expires,
                            next: now.saturating_add(100).min(expires),
                        };
                    }
                }
                Phase::ConfirmIdle { expires, at } => {
                    if now < at {
                        return Effects {
                            next_deadline: Some(at),
                            ..Default::default()
                        };
                    }
                    if now >= expires {
                        self.phase = Phase::ExitNext;
                    } else if snapshot.state == CombatState::Idle {
                        return self.after_exit(true);
                    } else {
                        self.phase = Phase::Idle {
                            expires,
                            next: now.saturating_add(100).min(expires),
                        };
                    }
                }
                Phase::Pending {
                    action: Pending::CellReady { jump, .. },
                    ..
                } => {
                    return Effects {
                        next_deadline: Some(jump.expires),
                        ..Default::default()
                    };
                }
                Phase::Pending { .. } | Phase::Done => return Effects::default(),
            }
        }
        Effects {
            next_deadline: Some(now),
            ..Default::default()
        }
    }
}
