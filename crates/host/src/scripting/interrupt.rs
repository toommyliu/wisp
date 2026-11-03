use std::io;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::scripting::alarm::Alarm;
use crate::scripting::cpu_time::ThreadCpu;
use crate::scripting::{Kill, RuntimeError};

const TURN_LIMIT: Duration = Duration::from_millis(200);
const WALL_BACKSTOP: Duration = Duration::from_secs(5);
const RETERMINATE: Duration = Duration::from_millis(20);

pub(crate) struct Interrupt {
    state: Mutex<State>,
    alarm: Alarm,
    stop_requested: AtomicBool,
}

struct State {
    isolate: Option<v8::IsolateHandle>,
    cpu: Option<ThreadCpu>,
    #[cfg(feature = "testing")]
    killed_turn_cpu: Option<Duration>,
    clock: Clock,
    stop: Option<Stop>,
    armed: Option<Instant>,
}

enum Clock {
    StartingScript,
    Idle,
    Running(Running),
    Blocked(Remaining),
}

#[derive(Clone, Copy)]
struct Running {
    cpu: Option<Duration>,
    fallback: Instant,
    backstop: Instant,
}

#[derive(Clone, Copy)]
struct Remaining {
    cpu: Option<Duration>,
    fallback: Duration,
    backstop: Duration,
}

impl Running {
    fn remaining(
        &mut self,
        now: Instant,
        read_cpu: impl FnOnce() -> io::Result<Duration>,
    ) -> Remaining {
        let cpu = self.cpu.and_then(|deadline| match read_cpu() {
            Ok(cpu) => Some(deadline.saturating_sub(cpu)),
            Err(error) => {
                tracing::warn!(%error, "thread CPU clock failed; using wall time for this turn");
                self.cpu = None;
                None
            }
        });
        Remaining {
            cpu,
            fallback: self.fallback.saturating_duration_since(now),
            backstop: self.backstop.saturating_duration_since(now),
        }
    }
}

impl Remaining {
    fn resume(self, now: Instant, read_cpu: impl FnOnce() -> Option<Duration>) -> Running {
        Running {
            cpu: self.cpu.and_then(|left| read_cpu().map(|cpu| cpu + left)),
            fallback: now + self.fallback,
            backstop: now + self.backstop,
        }
    }

    fn next(self) -> Result<Duration, Kill> {
        if self.backstop.is_zero() {
            return Err(Kill::WallBackstop);
        }
        let cpu = self.cpu.unwrap_or(self.fallback);
        if cpu.is_zero() {
            return Err(if self.cpu.is_some() {
                Kill::Watchdog
            } else {
                Kill::WatchdogFallback
            });
        }
        Ok(cpu.min(self.backstop))
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Stop {
    Kill(Kill),
    Shutdown,
}

impl Interrupt {
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            state: Mutex::new(State {
                isolate: None,
                cpu: None,
                #[cfg(feature = "testing")]
                killed_turn_cpu: None,
                clock: Clock::Idle,
                stop: None,
                armed: None,
            }),
            alarm: Alarm::new()?,
            stop_requested: AtomicBool::new(false),
        })
    }

    pub(crate) fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
    }

    pub(crate) fn stop_requested(&self) -> bool {
        self.stop_requested.load(Ordering::Acquire)
    }

    pub(crate) fn attach(&self, isolate: v8::IsolateHandle) {
        let mut state = self.state.lock().unwrap();
        if state.stop.is_some() {
            isolate.terminate_execution();
        }
        state.cpu = match ThreadCpu::current() {
            Ok(cpu) => Some(cpu),
            Err(error) => {
                tracing::warn!(%error, "could not capture thread CPU clock; using wall time");
                None
            }
        };
        state.isolate = Some(isolate);
    }

    pub(crate) fn detach(&self) {
        let mut state = self.state.lock().unwrap();
        state.isolate = None;
    }

    pub(crate) fn kill(&self, kill: Kill) {
        self.state.lock().unwrap().kill(kill);
    }

    pub(crate) fn shut_down(&self) {
        let mut state = self.state.lock().unwrap();
        state.stop = Some(Stop::Shutdown);
        if let Some(isolate) = &state.isolate {
            isolate.terminate_execution();
        }
        self.alarm.ring();
    }

    pub(crate) fn prepare_script(&self) {
        self.state.lock().unwrap().clock = Clock::StartingScript;
    }

    pub(crate) fn start_script(&self) {
        let mut state = self.state.lock().unwrap();
        if matches!(state.clock, Clock::StartingScript) {
            self.start_turn(&mut state);
        }
    }

    pub(crate) fn begin_turn(&self) {
        let mut state = self.state.lock().unwrap();
        if matches!(state.clock, Clock::StartingScript) {
            return;
        }
        self.start_turn(&mut state);
    }

    pub(crate) fn block(&self) {
        let mut state = self.state.lock().unwrap();
        if let Clock::Running(mut running) = state.clock {
            let left = running.remaining(Instant::now(), || state.read_cpu());
            state.clock = Clock::Blocked(left);
        }
    }

    pub(crate) fn unblock(&self) {
        let mut state = self.state.lock().unwrap();
        if let Clock::Blocked(left) = state.clock {
            let now = Instant::now();
            let running = left.resume(now, || state.read_cpu_or_warn());
            let left = Remaining {
                cpu: running.cpu.and(left.cpu),
                ..left
            };
            state.clock = Clock::Running(running);
            self.rouse(&state, now + left.next().unwrap_or_default());
        }
    }

    pub(crate) fn end_turn(&self) -> Option<Stop> {
        let mut state = self.state.lock().unwrap();
        #[cfg(feature = "testing")]
        if matches!(state.stop, Some(Stop::Kill(_))) {
            state.killed_turn_cpu = match state.clock {
                Clock::Running(running) => running.cpu.and_then(|deadline| {
                    state
                        .read_cpu()
                        .ok()
                        .map(|cpu| (cpu + TURN_LIMIT).saturating_sub(deadline))
                }),
                Clock::Blocked(left) => left.cpu.map(|left| TURN_LIMIT.saturating_sub(left)),
                _ => None,
            };
        }
        if !matches!(state.clock, Clock::StartingScript) {
            state.clock = Clock::Idle;
        }
        state.stop
    }

    pub(crate) fn stop(&self) -> Option<Stop> {
        self.state.lock().unwrap().stop
    }

    #[cfg(feature = "testing")]
    pub(crate) fn killed_turn_cpu_time(&self) -> Option<Duration> {
        self.state.lock().unwrap().killed_turn_cpu
    }

    fn start_turn(&self, state: &mut State) {
        let now = Instant::now();
        state.clock = Clock::Running(
            Remaining {
                cpu: state.cpu.as_ref().map(|_| TURN_LIMIT),
                fallback: TURN_LIMIT,
                backstop: WALL_BACKSTOP,
            }
            .resume(now, || state.read_cpu_or_warn()),
        );
        self.rouse(state, now + TURN_LIMIT);
    }

    fn rouse(&self, state: &State, deadline: Instant) {
        if state.armed.is_none_or(|armed| deadline < armed) {
            self.alarm.ring();
        }
    }

    pub(crate) fn watch(&self) {
        let mut state = self.state.lock().unwrap();
        loop {
            let until = match state.stop {
                Some(Stop::Shutdown) => return,
                Some(Stop::Kill(kill)) => {
                    state.kill(kill);
                    Some(Instant::now() + RETERMINATE)
                }
                None => match state.clock {
                    Clock::Running(mut running) => {
                        let now = Instant::now();
                        let remaining = running.remaining(now, || state.read_cpu());
                        state.clock = Clock::Running(running);
                        match remaining.next() {
                            Ok(left) => Some(now + left),
                            Err(kill) => {
                                state.kill(kill);
                                continue;
                            }
                        }
                    }
                    Clock::StartingScript | Clock::Idle | Clock::Blocked(_) => None,
                },
            };
            state.armed = until;
            state = self.alarm.wait(&self.state, state, until);
            state.armed = None;
        }
    }
}

impl State {
    fn read_cpu(&self) -> io::Result<Duration> {
        self.cpu.as_ref().unwrap().elapsed()
    }

    fn read_cpu_or_warn(&self) -> Option<Duration> {
        match self.read_cpu() {
            Ok(cpu) => Some(cpu),
            Err(error) => {
                tracing::warn!(%error, "thread CPU clock failed; using wall time for this turn");
                None
            }
        }
    }

    fn kill(&mut self, kill: Kill) {
        let Some(isolate) = &self.isolate else {
            return;
        };
        if self.stop.is_none() {
            self.stop = Some(Stop::Kill(kill));
        }
        isolate.terminate_execution();
    }
}

impl From<Stop> for RuntimeError {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::Kill(kill) => RuntimeError::Killed(kill),
            Stop::Shutdown => RuntimeError::Stopped,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/common/logs.rs"]
mod logs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_call_pauses_both_budgets() {
        let began = Instant::now();
        let mut running = Remaining {
            cpu: Some(TURN_LIMIT),
            fallback: TURN_LIMIT,
            backstop: WALL_BACKSTOP,
        }
        .resume(began, || Some(Duration::from_secs(1)));
        let blocked = began + Duration::from_millis(100);
        let left = running.remaining(blocked, || Ok(Duration::from_millis(1040)));
        let resumed = blocked + Duration::from_secs(6);
        let mut running = left.resume(resumed, || Some(Duration::from_millis(1070)));
        let remaining = running.remaining(resumed + Duration::from_millis(80), || {
            Ok(Duration::from_millis(1110))
        });
        assert_eq!(remaining.cpu, Some(Duration::from_millis(120)));
        assert_eq!(remaining.fallback, Duration::from_millis(20));
        assert_eq!(remaining.backstop, Duration::from_millis(4820));
        assert_eq!(remaining.next(), Ok(Duration::from_millis(120)));
    }

    #[test]
    fn a_failed_cpu_read_uses_the_turns_remaining_wall_budget_and_logs_once() {
        let logs = logs::Logs::default();
        let dispatch = logs.dispatch();
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let began = Instant::now();
        let mut running = Remaining {
            cpu: Some(TURN_LIMIT),
            fallback: TURN_LIMIT,
            backstop: WALL_BACKSTOP,
        }
        .resume(began, || Some(Duration::from_secs(1)));
        assert_eq!(
            running
                .remaining(began + Duration::from_millis(150), || {
                    Err(io::Error::other("CPU clock unavailable"))
                })
                .next(),
            Ok(Duration::from_millis(50))
        );
        assert_eq!(
            running
                .remaining(began + Duration::from_millis(200), || {
                    panic!("a failed clock must not be read again in the same turn")
                })
                .next(),
            Err(Kill::WatchdogFallback)
        );
        assert_eq!(
            logs.messages(tracing::Level::WARN),
            vec!["thread CPU clock failed; using wall time for this turn"]
        );
    }
}
