mod driver;
mod movement;
mod types;
pub use driver::FollowerPolicy;
pub(crate) use types::*;
#[cfg(test)]
mod tests;

use crate::game::travel;
use movement::Movement;

#[derive(Debug)]
enum Pending {
    Exit,
    GoTo,
    Jump { fallback: Option<usize> },
    Join { index: usize },
    Walk { revision: u64 },
}
#[derive(Debug)]
enum Stage {
    Dormant,
    Cycle,
    Pending {
        id: RequestId,
        kind: Pending,
        expires: Option<u64>,
    },
    GoTo {
        expires: u64,
    },
    Fallback {
        index: usize,
    },
    LateRoomFull {
        index: usize,
        until: u64,
    },
    FallbackVisible {
        index: usize,
    },
    Sleeping {
        until: u64,
    },
    Backoff {
        until: u64,
    },
}
pub(crate) struct Follower {
    config: Config,
    status: Status,
    stage: Stage,
    sequence: u64,
    combat: Option<RequestId>,
    engaged: bool,
    denied_target: bool,
    movement: Movement,
    target_present: bool,
    wake_deferred: bool,
    retry_delay: u64,
    watchdog: u64,
}
impl Follower {
    pub fn new(mut config: Config) -> Self {
        config.target_name = config.target_name.trim().to_lowercase();
        Self {
            config,
            status: Status::default(),
            stage: Stage::Dormant,
            sequence: 0,
            combat: None,
            engaged: false,
            denied_target: false,
            movement: Movement::default(),
            target_present: false,
            wake_deferred: false,
            retry_delay: 500,
            watchdog: 0,
        }
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    pub fn target_name(&self) -> &str {
        &self.config.target_name
    }
    pub fn restore(&mut self, status: Status) {
        if !self.status.running {
            self.status = Status {
                running: false,
                ..status
            };
        }
    }
    pub fn configure(&mut self, mut config: Config) -> bool {
        if self.status.running {
            return false;
        }
        config.target_name = config.target_name.trim().to_lowercase();
        self.config = config;
        self.status.last_error = None;
        self.status.stopped_reason = None;
        self.status.warnings.clear();
        true
    }
    fn id(&mut self) -> RequestId {
        self.sequence += 1;
        RequestId(self.sequence)
    }
    fn attempts(&self) -> u32 {
        if self.config.retry_enabled {
            self.config.max_attempts.get()
        } else {
            0
        }
    }
    fn cancel_pending(&mut self, effects: &mut Vec<Effect>) {
        if let Stage::Pending { id, .. } = self.stage {
            effects.push(Effect::Cancel { id });
        }
    }
    fn stop(&mut self, reason: &str, error: Option<String>, effects: &mut Vec<Effect>) {
        self.cancel_pending(effects);
        self.combat = None;
        effects.push(Effect::StopCombat);
        self.engaged = false;
        self.status.enabled = false;
        self.status.running = false;
        self.status.phase = Phase::Stopped;
        self.status.last_error = error;
        self.status.stopped_reason = Some(reason.into());
        self.status.warnings.clear();
        self.stage = Stage::Dormant;
        self.movement = Movement::default();
        self.denied_target = false;
        self.wake_deferred = false;
    }
    fn engage(&mut self, engaged: bool, effects: &mut Vec<Effect>) {
        if let Some(id) = self.combat
            && self.engaged != engaged
        {
            self.engaged = engaged;
            effects.push(Effect::EngageCombat { id, engaged });
        }
    }
    fn issue(&mut self, action: Action, kind: Pending, now: u64, effects: &mut Vec<Effect>) {
        self.engage(false, effects);
        let expires =
            matches!(kind, Pending::GoTo | Pending::Walk { .. }).then(|| now.saturating_add(5000));
        let id = self.id();
        self.stage = Stage::Pending { id, kind, expires };
        effects.push(Effect::Issue { id, action });
    }
    fn at_target(&self, snapshot: &Snapshot) -> bool {
        snapshot.me.as_ref().is_some_and(|me| {
            me.username.eq_ignore_ascii_case(&self.config.target_name)
                || snapshot
                    .target
                    .as_ref()
                    .is_some_and(|target| me.location.same(&target.location))
        })
    }
    fn success(&mut self, now: u64, effects: &mut Vec<Effect>) {
        self.status.attempts_remaining = self.attempts();
        self.status.last_error = None;
        self.status.stopped_reason = None;
        self.retry_delay = 500;
        self.engage(true, effects);
        let until = if self.wake_deferred {
            now
        } else {
            now.saturating_add(5000)
        };
        self.wake_deferred = false;
        self.watchdog = now.saturating_add(5000);
        self.stage = Stage::Sleeping { until };
    }
    fn failure(&mut self, error: String, reason: &str, now: u64, effects: &mut Vec<Effect>) {
        self.status.last_error = Some(error.clone());
        if self.config.retry_enabled {
            self.status.attempts_remaining = self.status.attempts_remaining.saturating_sub(1);
            if self.status.attempts_remaining > 0 {
                self.stage = Stage::Backoff {
                    until: now.saturating_add(self.retry_delay),
                };
                self.retry_delay = (self.retry_delay * 2).min(8000);
                return;
            }
        }
        self.stop(reason, Some(error), effects);
    }
    fn missing(&mut self, now: u64, effects: &mut Vec<Effect>) {
        let suffix = if self.config.locked_zone_fallbacks.is_empty() {
            ""
        } else {
            " in configured locked-zone locations"
        };
        self.failure(
            format!("Could not find {}{suffix}", self.config.target_name),
            "Target not found",
            now,
            effects,
        );
    }
    fn fallback(&mut self, error: Option<String>, now: u64, effects: &mut Vec<Effect>) {
        if self.config.retry_enabled && !self.config.locked_zone_fallbacks.is_empty() {
            self.stage = Stage::Fallback { index: 0 };
        } else if let Some(error) = error {
            self.failure(error, "Failed to follow target", now, effects);
        } else {
            self.missing(now, effects);
        }
    }
    fn follow_visible(&mut self, snapshot: &Snapshot, now: u64, effects: &mut Vec<Effect>) {
        self.denied_target = false;
        if let Some(target) = &snapshot.target {
            self.movement.seed(target);
            if self.config.copy_walk
                && let Some((position, revision)) = self.movement.pending
            {
                self.status.phase = Phase::Walking;
                self.issue(
                    Action::Walk(position),
                    Pending::Walk { revision },
                    now,
                    effects,
                );
                return;
            }
        }
        self.success(now, effects);
    }
    fn jump(
        &mut self,
        target: &Player,
        fallback: Option<usize>,
        now: u64,
        effects: &mut Vec<Effect>,
    ) {
        self.status.phase = Phase::Following;
        self.issue(
            Action::Travel(travel::Request::Jump {
                cell: target.location.cell.clone(),
                pad: Some(target.location.pad.clone()),
            }),
            Pending::Jump { fallback },
            now,
            effects,
        );
    }
    fn wake(&mut self, effects: &mut Vec<Effect>) {
        self.cancel_pending(effects);
        self.stage = Stage::Cycle;
        self.retry_delay = 500;
        self.wake_deferred = false;
    }
    fn traveling(&self) -> bool {
        matches!(
            self.stage,
            Stage::Pending { .. }
                | Stage::GoTo { .. }
                | Stage::LateRoomFull { .. }
                | Stage::Fallback { .. }
                | Stage::FallbackVisible { .. }
        )
    }
    fn server_message(
        &mut self,
        snapshot: &Snapshot,
        command: &str,
        message: &str,
        now: u64,
        effects: &mut Vec<Effect>,
    ) {
        let message = message.trim().to_lowercase();
        let denied = (command == "warning" && message == "cannot goto player in a locked zone.")
            || (command == "server"
                && message == format!("{} is ignoring goto requests.", self.config.target_name));
        if denied
            && matches!(
                self.stage,
                Stage::Pending {
                    kind: Pending::GoTo,
                    ..
                } | Stage::GoTo { .. }
            )
        {
            self.cancel_pending(effects);
            self.stage = Stage::Cycle;
            if self.at_target(snapshot) {
                self.follow_visible(snapshot, now, effects);
            } else {
                self.denied_target = true;
                self.fallback(None, now, effects);
            }
        } else if command == "warning" && message == "room join failed, destination room is full." {
            let index = match self.stage {
                Stage::Pending {
                    kind: Pending::Join { index },
                    ..
                } => Some(index),
                Stage::LateRoomFull { index, until } if now <= until => Some(index),
                _ => None,
            };
            if let Some(index) = index {
                self.cancel_pending(effects);
                self.stage = Stage::Fallback { index: index + 1 };
            }
        }
    }
    fn deadline(&self, now: u64) -> Option<u64> {
        if !self.status.running {
            return None;
        }
        let next = match self.stage {
            Stage::Cycle | Stage::Fallback { .. } | Stage::FallbackVisible { .. } => now,
            Stage::Pending { expires, .. } => expires.unwrap_or(self.watchdog),
            Stage::GoTo { expires } => now.saturating_add(100).min(expires),
            Stage::LateRoomFull { until, .. }
            | Stage::Sleeping { until }
            | Stage::Backoff { until } => until,
            Stage::Dormant => return None,
        };
        Some(next.min(self.watchdog))
    }
    pub fn advance(&mut self, snapshot: &Snapshot, event: Event, now: u64) -> Effects {
        let mut effects = vec![];
        match event {
            Event::Start => {
                if self.status.running {
                    self.stop("Stopped by user", None, &mut effects);
                }
                self.movement = Movement::default();
                self.denied_target = false;
                self.wake_deferred = false;
                self.retry_delay = 500;
                if self.config.target_name.is_empty() {
                    self.stop(
                        "Target not found",
                        Some("Target name is required".into()),
                        &mut effects,
                    );
                } else {
                    self.status = Status {
                        enabled: true,
                        running: true,
                        phase: Phase::Starting,
                        attempts_remaining: self.attempts(),
                        ..Status::default()
                    };
                    self.stage = Stage::Cycle;
                    self.watchdog = now.saturating_add(5000);
                    self.target_present = snapshot.target.is_some();
                    if self.config.combat_enabled {
                        let id = self.id();
                        self.combat = Some(id);
                        effects.push(Effect::StartCombat {
                            id,
                            priorities: self.config.attack_priority.clone(),
                            fallback_rank: self.config.attack_priority.len(),
                        });
                    }
                }
                return Effects {
                    next_deadline: self.deadline(now),
                    actions: effects,
                };
            }
            Event::Stop => {
                if self.status.running {
                    self.stop("Stopped by user", None, &mut effects);
                }
                self.status.attempts_remaining = 3;
                self.status.last_error = None;
                self.status.phase = Phase::Stopped;
                self.status.stopped_reason = Some("Stopped by user".into());
                self.status.warnings.clear();
                return Effects {
                    actions: effects,
                    next_deadline: None,
                };
            }
            _ => {}
        }
        if !self.status.running {
            return Effects::default();
        }
        if now >= self.watchdog {
            self.watchdog = now.saturating_add(5000);
        }
        let mut completion = None;
        match event {
            Event::Completed { id, reply } => {
                if matches!(self.stage, Stage::Pending { id: expected, .. } if id == expected) {
                    completion = Some(reply);
                } else {
                    return Effects {
                        actions: effects,
                        next_deadline: self.deadline(now),
                    };
                }
            }
            Event::Combat { id, event } => {
                if self.combat == Some(id) {
                    match event {
                        CombatEvent::Attacking if self.engaged => self.status.phase = Phase::Combat,
                        CombatEvent::Failed(message) => self.stop(
                            "Combat profile failed",
                            Some(nonempty(message, "Combat profile failed")),
                            &mut effects,
                        ),
                        CombatEvent::Warnings(warnings) => self.status.warnings = warnings,
                        _ => {}
                    }
                }
                return Effects {
                    actions: effects,
                    next_deadline: self.deadline(now),
                };
            }
            Event::TargetLocation {
                username,
                location,
                kind,
            } => {
                if username.eq_ignore_ascii_case(&self.config.target_name)
                    && self.movement.observe(location, kind, self.config.copy_walk)
                {
                    self.wake(&mut effects);
                }
            }
            Event::OwnMapJoined | Event::ConnectionChanged => {
                self.movement = Movement::default();
                let joining = matches!(
                    self.stage,
                    Stage::Pending {
                        kind: Pending::Join { .. },
                        ..
                    } | Stage::LateRoomFull { .. }
                );
                if !joining || matches!(event, Event::ConnectionChanged) {
                    self.wake(&mut effects);
                }
            }
            Event::PlayersChanged => {
                let present = snapshot.target.is_some();
                if self.target_present != present || !self.traveling() {
                    self.wake(&mut effects);
                } else {
                    self.wake_deferred = true;
                }
                self.target_present = present;
            }
            Event::ReadinessChanged => {
                if self.traveling() {
                    self.wake_deferred = true;
                } else {
                    self.wake(&mut effects);
                }
            }
            Event::ServerMessage { command, message } => {
                self.server_message(snapshot, &command, &message, now, &mut effects)
            }
            _ => {}
        }
        for _ in 0..8 {
            let stage = std::mem::replace(&mut self.stage, Stage::Dormant);
            match stage {
                Stage::Cycle => {
                    if !snapshot.can_act
                        || snapshot.me.as_ref().is_none_or(|me| {
                            me.username.eq_ignore_ascii_case(&self.config.target_name)
                        })
                    {
                        self.success(now, &mut effects);
                    } else if let Some(target) = &snapshot.target {
                        self.denied_target = false;
                        self.movement.seed(target);
                        if self.at_target(snapshot) {
                            self.follow_visible(snapshot, now, &mut effects);
                        } else {
                            self.jump(target, None, now, &mut effects);
                        }
                    } else {
                        self.status.phase = Phase::Following;
                        self.issue(
                            Action::Travel(travel::Request::Exit),
                            Pending::Exit,
                            now,
                            &mut effects,
                        );
                    }
                    break;
                }
                Stage::Pending { id, kind, expires } => {
                    let reply = if expires.is_some_and(|expires| now >= expires) {
                        Some(Reply::Error(String::new()))
                    } else {
                        completion.take()
                    };
                    let Some(reply) = reply else {
                        self.stage = Stage::Pending { id, kind, expires };
                        break;
                    };
                    if expires.is_some_and(|expires| now >= expires) {
                        effects.push(Effect::Cancel { id });
                    }
                    let returned = matches!(reply, Reply::Returned(true));
                    match kind {
                        Pending::Exit => {
                            if self.denied_target {
                                self.fallback(None, now, &mut effects);
                            } else {
                                self.issue(
                                    Action::GoTo(self.config.target_name.clone()),
                                    Pending::GoTo,
                                    now,
                                    &mut effects,
                                );
                            }
                        }
                        Pending::GoTo => {
                            if self.at_target(snapshot) {
                                self.follow_visible(snapshot, now, &mut effects);
                            } else if let Reply::Error(message) = reply {
                                self.fallback(
                                    Some(nonempty(message, "Failed to go to target")),
                                    now,
                                    &mut effects,
                                );
                            } else {
                                let timeout = if self.config.retry_enabled
                                    && !self.config.locked_zone_fallbacks.is_empty()
                                {
                                    3000
                                } else {
                                    10000
                                };
                                self.stage = Stage::GoTo {
                                    expires: now.saturating_add(timeout),
                                };
                            }
                        }
                        Pending::Join { index } => {
                            self.stage = if returned {
                                Stage::LateRoomFull {
                                    index,
                                    until: now.saturating_add(100),
                                }
                            } else {
                                Stage::Fallback { index: index + 1 }
                            };
                        }
                        Pending::Jump { fallback } => {
                            if returned && self.at_target(snapshot) {
                                self.follow_visible(snapshot, now, &mut effects);
                            } else if let Some(index) = fallback {
                                self.stage = Stage::Fallback { index: index + 1 };
                            } else {
                                let error = match reply {
                                    Reply::Error(message) => {
                                        nonempty(message, "Failed to follow target")
                                    }
                                    _ => format!("Could not reach {}", self.config.target_name),
                                };
                                self.failure(error, "Failed to follow target", now, &mut effects);
                            }
                        }
                        Pending::Walk { revision } => {
                            if returned {
                                self.movement.complete(revision);
                            }
                            self.success(now, &mut effects);
                        }
                    }
                    if !matches!(self.stage, Stage::Fallback { .. }) {
                        break;
                    }
                }
                Stage::GoTo { expires } => {
                    if self.at_target(snapshot) {
                        self.follow_visible(snapshot, now, &mut effects);
                        break;
                    }
                    if now >= expires {
                        self.fallback(None, now, &mut effects);
                    } else {
                        self.stage = Stage::GoTo { expires };
                        break;
                    }
                }
                Stage::Fallback { index } => {
                    if let Some(map) = self.config.locked_zone_fallbacks.get(index) {
                        let destination = fallback_map(map, &self.config.locked_zone_room_override);
                        self.status.phase = Phase::Following;
                        self.issue(
                            Action::Travel(travel::Request::Join {
                                target: destination,
                                cell: None,
                                pad: None,
                                random_room: snapshot.random_room,
                            }),
                            Pending::Join { index },
                            now,
                            &mut effects,
                        );
                    } else {
                        self.failure(
                            "Destination room is full or unreachable".into(),
                            "Room join failed",
                            now,
                            &mut effects,
                        );
                    }
                    break;
                }
                Stage::LateRoomFull { index, until } => {
                    if now < until {
                        self.stage = Stage::LateRoomFull { index, until };
                        break;
                    }
                    self.stage = Stage::FallbackVisible { index };
                }
                Stage::FallbackVisible { index } => {
                    if self.at_target(snapshot) {
                        self.follow_visible(snapshot, now, &mut effects);
                        break;
                    }
                    if let (Some(_), Some(target)) = (&snapshot.me, &snapshot.target) {
                        self.jump(target, Some(index), now, &mut effects);
                        break;
                    }
                    self.stage = Stage::Fallback { index: index + 1 };
                }
                Stage::Sleeping { until } | Stage::Backoff { until } if now >= until => {
                    self.stage = Stage::Cycle
                }
                other => {
                    self.stage = other;
                    break;
                }
            }
        }
        Effects {
            next_deadline: self.deadline(now),
            actions: effects,
        }
    }
}
fn nonempty(message: String, fallback: &str) -> String {
    if message.is_empty() {
        fallback.into()
    } else {
        message
    }
}
fn fallback_map(map: &str, room_override: &str) -> String {
    let map = map.trim();
    let room = room_override.trim();
    if map.is_empty() || room.is_empty() || map.contains('-') {
        map.into()
    } else {
        format!("{map}-{room}")
    }
}
