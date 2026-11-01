#![allow(dead_code, reason = "This module has no controller caller.")]

pub mod queue;
mod types;
pub use queue::{Command as QueueCommand, QueuePhase};
pub use types::*;
#[cfg(test)]
mod queue_tests;
#[cfg(test)]
mod tests;

use queue::{Action as QueueAction, Queue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Standalone,
    Queue,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attempt {
    Initial,
    Restart,
}
#[derive(Clone, Debug)]
struct Run {
    id: RunId,
    binding: Binding,
    request: Request,
    username: String,
    source: Source,
    attempt: Attempt,
}
#[derive(Clone, Copy, Debug)]
enum PlayerRead {
    Scheduled(u64),
    Requested,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Worker {
    Reaping,
    Done,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cleanup {
    BeforeCanceled,
    Canceled(SafePhase),
    SafeMove,
    Logout,
    Done,
}
#[derive(Debug)]
enum State {
    Idle,
    Waiting {
        run: Run,
        deadline: u64,
        read: PlayerRead,
    },
    SafeBefore(Run),
    Booting(Run),
    Running(Run),
    Stopping {
        run: Run,
        deadline: u64,
        reason: String,
    },
    Finishing {
        run: Run,
        worker: Worker,
        cleanup: Cleanup,
        outcome: Outcome,
    },
}
impl State {
    fn run(&self) -> Option<&Run> {
        match self {
            Self::Idle => None,
            Self::Waiting { run, .. }
            | Self::SafeBefore(run)
            | Self::Booting(run)
            | Self::Running(run)
            | Self::Stopping { run, .. }
            | Self::Finishing { run, .. } => Some(run),
        }
    }
    fn phase(&self) -> Option<RunPhase> {
        match self {
            Self::Idle => None,
            Self::Waiting { .. } => Some(RunPhase::Waiting),
            Self::SafeBefore(_) => Some(RunPhase::SafeBefore),
            Self::Booting(_) => Some(RunPhase::Booting),
            Self::Running(_) => Some(RunPhase::Running),
            Self::Stopping { .. } => Some(RunPhase::Stopping),
            Self::Finishing { .. } => Some(RunPhase::Finishing),
        }
    }
}
#[derive(Debug)]
enum Pending {
    Queue(Entry),
    Restart {
        request: Request,
        username: String,
        source: Source,
    },
}
#[derive(Clone, Copy, Debug)]
enum Connection {
    Up(Binding),
    Down(Binding),
}
impl Connection {
    fn binding(self) -> Binding {
        match self {
            Self::Up(binding) | Self::Down(binding) => binding,
        }
    }
}
#[derive(Debug)]
pub struct Lifecycle {
    state: State,
    queue: Queue,
    pending: Option<Pending>,
    connection: Connection,
    sequence: u64,
    latest: Option<Terminal>,
}
impl Lifecycle {
    pub fn new(binding: Binding) -> Self {
        Self {
            state: State::Idle,
            queue: Queue::default(),
            pending: None,
            connection: Connection::Up(binding),
            sequence: 0,
            latest: None,
        }
    }
    pub fn status(&self) -> Status {
        Status {
            active: self.state.run().map(|run| ActiveStatus {
                run: run.id,
                name: run.request.name.clone(),
                phase: self.state.phase().unwrap(),
            }),
            waiting_to_restart: matches!(self.pending, Some(Pending::Restart { .. }))
                || matches!(&self.state, State::Waiting { run, .. } if run.attempt == Attempt::Restart),
            latest: self.latest.clone(),
            queue: self.queue.status(),
        }
    }
    pub fn advance(
        &mut self,
        event: Event,
        readiness: &Readiness,
        options: &Options,
        now: u64,
    ) -> Transition {
        let before = self.status();
        let mut result = Transition::default();
        self.expire(options, now, &mut result.effects);
        match event {
            Event::Command(Command::Start(request)) => {
                if !matches!(self.state, State::Idle)
                    || self.pending.is_some()
                    || self.queue.status().phase != QueuePhase::Idle
                {
                    result.rejection = Some(Rejection::Busy);
                } else {
                    result.rejection = self
                        .begin(
                            request,
                            Source::Standalone,
                            Attempt::Initial,
                            readiness,
                            now,
                            &mut result.effects,
                        )
                        .err();
                }
            }
            Event::Command(Command::Queue(command)) => {
                if matches!(command, QueueCommand::Start)
                    && (!matches!(self.state, State::Idle) || self.pending.is_some())
                {
                    result.rejection = Some(Rejection::Busy);
                } else {
                    match self.queue.command(command) {
                        Ok(actions) => {
                            self.queue_actions(actions, options, now, &mut result.effects)
                        }
                        Err(error) => result.rejection = Some(error),
                    }
                }
            }
            Event::Prepared { id, result: answer } => {
                let actions = self.queue.prepared(id, answer);
                self.queue_actions(actions, options, now, &mut result.effects);
            }
            Event::Command(Command::Stop { run, reason }) => {
                if self.state.run().is_some_and(|active| active.id == run) {
                    self.stop(reason, options, now, &mut result.effects);
                }
            }
            Event::Command(Command::Kill { run }) => {
                if self.state.run().is_some_and(|active| active.id == run) {
                    self.finish(
                        Outcome::Killed(KillReason::Requested),
                        true,
                        options,
                        &mut result.effects,
                    );
                }
            }
            Event::PlayerReady { run: id, ready } => {
                if let State::Waiting {
                    run,
                    read,
                    deadline,
                } = &mut self.state
                    && run.id == id
                    && matches!(read, PlayerRead::Requested)
                {
                    if ready
                        && readiness.ready()
                        && matches!(self.connection, Connection::Up(_))
                        && (run.attempt != Attempt::Restart || options.restart_after_reconnect)
                    {
                        let run = run.clone();
                        if options.safe_start_stop {
                            result.effects.push(Effect::SafeMove {
                                run: id,
                                binding: run.binding,
                                phase: SafePhase::Before,
                                room_policy: options.room_policy.clone(),
                            });
                            self.state = State::SafeBefore(run);
                        } else {
                            self.spawn(run, &mut result.effects);
                        }
                    } else {
                        let next = now.saturating_add(100).min(*deadline);
                        *read = PlayerRead::Scheduled(next);
                        result.effects.push(Effect::ScheduleDeadline(next));
                    }
                }
            }
            Event::RunReady(id) => {
                if let State::Booting(run) = &self.state
                    && run.id == id
                {
                    self.state = State::Running(run.clone());
                }
            }
            Event::RunExited { run: id, outcome } => {
                if matches!(&self.state, State::Booting(run) | State::Running(run) | State::Stopping { run, .. } if run.id == id)
                {
                    let outcome = match (&self.state, outcome) {
                        (State::Stopping { reason, .. }, outcome)
                            if !matches!(outcome, Outcome::Killed(_)) =>
                        {
                            Outcome::Stopped(StopReason::Requested(reason.clone()))
                        }
                        (_, outcome) => outcome,
                    };
                    self.finish(outcome, false, options, &mut result.effects);
                }
            }
            Event::Reaped(id) => {
                if let State::Finishing { run, worker, .. } = &mut self.state
                    && run.id == id
                {
                    *worker = Worker::Done;
                }
            }
            Event::SafeMoveDone { run: id, phase } => match (&mut self.state, phase) {
                (State::Finishing { run, cleanup, .. }, phase)
                    if run.id == id && *cleanup == Cleanup::Canceled(phase) =>
                {
                    *cleanup = Cleanup::Done;
                }

                (
                    State::Finishing {
                        run,
                        cleanup,
                        outcome,
                        ..
                    },
                    SafePhase::Before,
                ) if run.id == id && *cleanup == Cleanup::BeforeCanceled => {
                    *cleanup = if matches!(
                        outcome,
                        Outcome::Stopped(StopReason::AccountChanged | StopReason::PlayerReplaced)
                    ) {
                        Cleanup::Done
                    } else {
                        Self::cleanup(run, outcome, options, &mut result.effects)
                    };
                }

                (State::SafeBefore(run), SafePhase::Before) if run.id == id => {
                    let run = run.clone();
                    self.spawn(run, &mut result.effects);
                }
                (
                    State::Finishing {
                        run,
                        cleanup,
                        outcome,
                        ..
                    },
                    SafePhase::After,
                ) if run.id == id && *cleanup == Cleanup::SafeMove => {
                    if matches!(outcome, Outcome::Exited { logout: true, .. }) {
                        *cleanup = Cleanup::Logout;
                        result.effects.push(Effect::Logout {
                            run: id,
                            binding: run.binding,
                        });
                    } else {
                        *cleanup = Cleanup::Done;
                    }
                }
                _ => {}
            },
            Event::LogoutDone(id) => {
                if let State::Finishing { run, cleanup, .. } = &mut self.state
                    && run.id == id
                    && *cleanup == Cleanup::Logout
                {
                    *cleanup = Cleanup::Done;
                }
            }
            Event::ConnectionLost(binding) if self.connection.binding() == binding => {
                self.connection = Connection::Down(binding);
                if let Some(run) = self.state.run()
                    && !matches!(self.state, State::Finishing { .. })
                {
                    if options.restart_after_reconnect
                        && !run.username.is_empty()
                        && !matches!(self.state, State::Stopping { .. })
                    {
                        self.pending = Some(Pending::Restart {
                            request: run.request.clone(),
                            username: run.username.clone(),
                            source: run.source,
                        });
                    }
                    self.finish(
                        Outcome::Stopped(StopReason::ConnectionLost),
                        true,
                        options,
                        &mut result.effects,
                    );
                }
            }
            Event::ConnectionRestored(binding) => {
                let current = self.connection.binding();
                if binding.player == current.player && binding.connection.0 >= current.connection.0
                {
                    self.connection = Connection::Up(binding);
                }
            }
            Event::AccountChanged => {
                self.cancel_restart();
                self.invalidate(StopReason::AccountChanged, options, &mut result.effects);
            }
            Event::PlayerReplaced(binding) => {
                self.cancel_restart();
                self.connection = Connection::Up(binding);
                self.invalidate(StopReason::PlayerReplaced, options, &mut result.effects);
            }
            Event::OptionsChanged
            | Event::ProjectionChanged
            | Event::Deadline
            | Event::ConnectionLost(_) => {}
        }
        if !options.restart_after_reconnect {
            self.cancel_restart();
            if matches!(&self.state, State::Waiting { run, .. } if run.attempt == Attempt::Restart)
            {
                self.finish(
                    Outcome::Stopped(StopReason::Requested(
                        "Restart after reconnect disabled".into(),
                    )),
                    false,
                    options,
                    &mut result.effects,
                );
            }
        }
        if let Some(Pending::Restart { username, .. }) = &self.pending
            && !readiness.username.trim().is_empty()
            && username != &readiness.username.trim().to_lowercase()
        {
            self.cancel_restart();
        }
        if matches!(
            self.state,
            State::Finishing {
                worker: Worker::Done,
                cleanup: Cleanup::Done,
                ..
            }
        ) {
            self.state = State::Idle;
        }
        if matches!(self.state, State::Idle) {
            let can_begin = match &self.pending {
                Some(Pending::Queue(_)) => true,
                Some(Pending::Restart { .. }) => {
                    matches!(self.connection, Connection::Up(_)) && readiness.ready()
                }
                None => false,
            };
            if can_begin {
                let pending = self.pending.take().unwrap();
                let (request, source, attempt) = match pending {
                    Pending::Queue(entry) => (entry.request, Source::Queue, Attempt::Initial),
                    Pending::Restart {
                        request, source, ..
                    } => (request, source, Attempt::Restart),
                };
                if let Err(error) = self.begin(
                    request,
                    source,
                    attempt,
                    readiness,
                    now,
                    &mut result.effects,
                ) {
                    result.rejection = Some(error);
                    if source == Source::Queue {
                        let actions = self
                            .queue
                            .terminal(Outcome::Failed("Run identity exhausted".into()));
                        self.queue_actions(actions, options, now, &mut result.effects);
                    }
                }
            }
        }
        if let State::Waiting {
            run,
            deadline,
            read,
        } = &mut self.state
            && matches!(read, PlayerRead::Scheduled(at) if now >= *at)
            && readiness.ready()
            && matches!(self.connection, Connection::Up(_))
        {
            run.binding = self.connection.binding();
            run.username = readiness.username.trim().to_lowercase();
            *read = PlayerRead::Requested;
            result.effects.push(Effect::ReadPlayerReady {
                run: run.id,
                binding: run.binding,
                deadline: *deadline,
            });
        }
        let after = self.status();
        if before != after {
            result.effects.push(Effect::Publish(after));
        }
        result
    }
    fn begin(
        &mut self,
        request: Request,
        source: Source,
        attempt: Attempt,
        readiness: &Readiness,
        now: u64,
        effects: &mut Vec<Effect>,
    ) -> Result<(), Rejection> {
        self.sequence = self.sequence.checked_add(1).ok_or(Rejection::IdExhausted)?;
        let run = Run {
            id: RunId(self.sequence),
            binding: self.connection.binding(),
            request,
            username: readiness.username.trim().to_lowercase(),
            source,
            attempt,
        };
        let deadline = now.saturating_add(10_000);
        self.state = State::Waiting {
            run,
            deadline,
            read: PlayerRead::Scheduled(now),
        };
        effects.push(Effect::ScheduleDeadline(deadline));
        Ok(())
    }
    fn spawn(&mut self, run: Run, effects: &mut Vec<Effect>) {
        effects.push(Effect::Spawn {
            run: run.id,
            binding: run.binding,
            request: run.request.clone(),
        });
        self.state = State::Booting(run);
    }
    fn stop(&mut self, reason: String, options: &Options, now: u64, effects: &mut Vec<Effect>) {
        match &self.state {
            State::Booting(run) | State::Running(run) => {
                let run = run.clone();
                let deadline = now.saturating_add(5000);
                effects.push(Effect::SignalStop { run: run.id });
                effects.push(Effect::ScheduleDeadline(deadline));
                self.state = State::Stopping {
                    run,
                    deadline,
                    reason,
                };
            }
            State::Waiting { .. } | State::SafeBefore(_) => self.finish(
                Outcome::Stopped(StopReason::Requested(reason)),
                false,
                options,
                effects,
            ),
            State::Idle | State::Stopping { .. } | State::Finishing { .. } => {}
        }
    }
    fn expire(&mut self, options: &Options, now: u64, effects: &mut Vec<Effect>) {
        match &self.state {
            State::Waiting { deadline, .. } if now >= *deadline => self.finish(
                Outcome::Failed("Script readiness timed out after 10 s".into()),
                false,
                options,
                effects,
            ),
            State::Stopping { deadline, .. } if now >= *deadline => self.finish(
                Outcome::Stopped(StopReason::GraceExpired),
                true,
                options,
                effects,
            ),
            _ => {}
        }
    }
    fn finish(
        &mut self,
        outcome: Outcome,
        kill: bool,
        options: &Options,
        effects: &mut Vec<Effect>,
    ) {
        if matches!(self.state, State::Idle | State::Finishing { .. }) {
            return;
        }
        let run = self.state.run().unwrap().clone();
        let worker = if matches!(
            self.state,
            State::Booting(_) | State::Running(_) | State::Stopping { .. }
        ) {
            if kill {
                effects.push(Effect::Kill { run: run.id });
            }
            Worker::Reaping
        } else {
            Worker::Done
        };
        if matches!(self.state, State::SafeBefore(_)) {
            effects.push(Effect::CancelSafeMove {
                run: run.id,
                phase: SafePhase::Before,
            });
        }
        effects.push(Effect::Revoke { run: run.id });
        let terminal = Terminal {
            run: run.id,
            name: run.request.name.clone(),
            outcome: outcome.clone(),
        };
        effects.push(Effect::Terminal(terminal.clone()));
        self.latest = Some(terminal);
        if worker == Worker::Reaping {
            effects.push(Effect::Reap { run: run.id });
        }
        let cleanup = if matches!(self.state, State::SafeBefore(_)) {
            Cleanup::BeforeCanceled
        } else {
            Self::cleanup(&run, &outcome, options, effects)
        };
        let source = run.source;
        self.state = State::Finishing {
            run,
            worker,
            cleanup,
            outcome: outcome.clone(),
        };
        if source == Source::Queue && !matches!(self.pending, Some(Pending::Restart { .. })) {
            let actions = self.queue.terminal(outcome);
            for action in actions {
                if let QueueAction::Launch(entry) = action {
                    self.pending = Some(Pending::Queue(entry));
                }
            }
        }
    }
    fn cleanup(
        run: &Run,
        outcome: &Outcome,
        options: &Options,
        effects: &mut Vec<Effect>,
    ) -> Cleanup {
        if options.safe_start_stop {
            effects.push(Effect::SafeMove {
                run: run.id,
                binding: run.binding,
                phase: SafePhase::After,
                room_policy: options.room_policy.clone(),
            });
            Cleanup::SafeMove
        } else if matches!(outcome, Outcome::Exited { logout: true, .. }) {
            effects.push(Effect::Logout {
                run: run.id,
                binding: run.binding,
            });
            Cleanup::Logout
        } else {
            Cleanup::Done
        }
    }
    fn queue_actions(
        &mut self,
        actions: Vec<QueueAction>,
        options: &Options,
        now: u64,
        effects: &mut Vec<Effect>,
    ) {
        for action in actions {
            match action {
                QueueAction::Prepare { id, entry } => effects.push(Effect::Prepare { id, entry }),
                QueueAction::Launch(entry) => self.pending = Some(Pending::Queue(entry)),
                QueueAction::Stop(reason) => {
                    if self.pending.take().is_some() {
                        self.queue
                            .terminal(Outcome::Stopped(StopReason::Requested(reason)));
                    } else {
                        self.stop(reason, options, now, effects);
                    }
                }
            }
        }
    }
    fn cancel_restart(&mut self) {
        if matches!(self.pending, Some(Pending::Restart { .. })) {
            let Some(Pending::Restart { source, .. }) = self.pending.take() else {
                unreachable!()
            };
            if source == Source::Queue {
                self.queue
                    .terminal(Outcome::Stopped(StopReason::ConnectionLost));
            }
        }
    }
    fn invalidate(&mut self, reason: StopReason, options: &Options, effects: &mut Vec<Effect>) {
        if let State::Finishing { run, cleanup, .. } = &mut self.state {
            match *cleanup {
                Cleanup::SafeMove => {
                    effects.push(Effect::CancelSafeMove {
                        run: run.id,
                        phase: SafePhase::After,
                    });
                    *cleanup = Cleanup::Canceled(SafePhase::After);
                }
                Cleanup::BeforeCanceled => *cleanup = Cleanup::Canceled(SafePhase::Before),
                Cleanup::Logout => *cleanup = Cleanup::Done,
                Cleanup::Canceled(_) | Cleanup::Done => {}
            }
        } else {
            self.finish(Outcome::Stopped(reason), true, options, effects);
        }
    }
}
