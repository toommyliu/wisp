mod alarm;
pub(crate) mod cpu_time;
mod engine;
mod host;
mod interrupt;
pub(crate) mod io;
mod modules;

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::task::{Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crate::ScriptLink;
use crate::bridge::Error as EventError;
use lucent_protocol::script::{Script, ScriptInputs, ScriptOutcome};
use serde::de::DeserializeOwned;

use engine::Engine;
use interrupt::{Interrupt, Stop};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Kill {
    #[error("a turn used more than 200 ms of CPU time")]
    Watchdog,
    #[error("a turn exceeded the 5 s wall-clock backstop")]
    WallBackstop,
    #[error("a turn exceeded 200 ms of wall time after its CPU clock failed")]
    WatchdogFallback,
    #[error("it reached the 64 MiB heap limit")]
    HeapLimit,
    #[error("it was stopped")]
    Requested,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("couldn't start the script thread: {0}")]
    Thread(#[from] std::io::Error),
    #[error("the script runtime has stopped")]
    Stopped,
    #[error("timed out waiting for the script runtime")]
    Timeout,
    #[error("script events overflowed and were lost")]
    EventsLost,
    #[error("the script was killed: {0}")]
    Killed(Kill),
    #[error("{0}")]
    JavaScript(String),
    #[error("couldn't decode the script result: {0}")]
    Decode(#[from] serde_json::Error),
}

pub struct Runtime {
    commands: mpsc::Sender<Command>,
    interrupt: Arc<Interrupt>,
    thread: Option<thread::JoinHandle<()>>,
    terminal: mpsc::Receiver<Result<(), RuntimeError>>,
    ready: mpsc::Receiver<()>,
}

enum Command {
    Io,
    Wake,
    StopScript,
    StartScript,
    Stop,
    Kill,
    Eval {
        name: String,
        source: String,
        reply: mpsc::Sender<Result<String, RuntimeError>>,
    },
}

struct IoWake(mpsc::Sender<Command>);

impl Wake for IoWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(Command::Io);
    }
}

struct EventWake {
    commands: mpsc::Sender<Command>,
    queued: AtomicBool,
}

impl Wake for EventWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if !self.queued.swap(true, Ordering::AcqRel) {
            let _ = self.commands.send(Command::Wake);
        }
    }
}

pub struct PendingEval(mpsc::Receiver<Result<String, RuntimeError>>);

impl PendingEval {
    pub fn wait<T: DeserializeOwned>(&self, timeout: Duration) -> Result<T, RuntimeError> {
        let json = self.0.recv_timeout(timeout).map_err(|error| match error {
            RecvTimeoutError::Timeout => RuntimeError::Timeout,
            RecvTimeoutError::Disconnected => RuntimeError::Stopped,
        })??;
        Ok(serde_json::from_str(&json)?)
    }
}

pub struct ScriptRun {
    runtime: Runtime,
    outcome: PendingEval,
}

enum Execution {
    Evals,
    Script,
    StartScript {
        script: Script,
        inputs: ScriptInputs,
        reply: mpsc::Sender<Result<String, RuntimeError>>,
    },
}

impl ScriptRun {
    pub fn start(
        link: ScriptLink,
        script: Script,
        inputs: ScriptInputs,
    ) -> Result<Self, RuntimeError> {
        Self::spawn(
            link,
            script,
            inputs,
            Waker::noop().clone(),
            Waker::noop().clone(),
        )
    }

    pub(crate) fn spawn(
        link: ScriptLink,
        script: Script,
        inputs: ScriptInputs,
        ready: Waker,
        exited: Waker,
    ) -> Result<Self, RuntimeError> {
        let (reply, pending) = mpsc::channel();
        let runtime = Runtime::spawn(
            link,
            Execution::StartScript {
                script,
                inputs,
                reply,
            },
            ready,
            exited,
        )?;
        Ok(Self {
            runtime,
            outcome: PendingEval(pending),
        })
    }

    pub(crate) fn take_ready(&self) -> bool {
        self.runtime.ready.try_recv().is_ok()
    }

    pub fn stop_script(&self) {
        let _ = self.runtime.commands.send(Command::StopScript);
    }

    pub fn kill(&self) {
        self.runtime.kill();
    }

    pub fn wait(&self, timeout: Duration) -> Result<ScriptOutcome, RuntimeError> {
        let began = Instant::now();
        match self.outcome.wait(timeout) {
            Err(RuntimeError::Stopped) => match self
                .runtime
                .terminal
                .recv_timeout(timeout.saturating_sub(began.elapsed()))
            {
                Ok(Err(error)) => Err(error),
                Err(RecvTimeoutError::Timeout) => Err(RuntimeError::Timeout),
                _ => Err(RuntimeError::Stopped),
            },
            result => result,
        }
    }

    #[cfg(feature = "testing")]
    pub fn join_cpu_time(&mut self) -> Option<Duration> {
        self.runtime.join_cpu_time()
    }
}

impl Runtime {
    pub fn start(link: ScriptLink) -> Result<Self, RuntimeError> {
        let runtime = Self::spawn(
            link,
            Execution::Evals,
            Waker::noop().clone(),
            Waker::noop().clone(),
        )?;
        runtime.wait_ready()?;
        Ok(runtime)
    }

    fn spawn(
        link: ScriptLink,
        execution: Execution,
        ready_notify: Waker,
        exited: Waker,
    ) -> Result<Self, RuntimeError> {
        let interrupt = Arc::new(Interrupt::new()?);
        let thread_interrupt = interrupt.clone();
        let (commands, incoming) = mpsc::channel();
        let (ready, boot) = mpsc::channel();
        let (finished, terminal) = mpsc::channel();
        let player = link.player_status();
        let wake = Arc::new(EventWake {
            commands: commands.clone(),
            queued: AtomicBool::new(false),
        });
        let io_wake = Waker::from(Arc::new(IoWake(commands.clone())));
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let thread = thread::Builder::new()
            .name(format!("lucent-js-{}", link.id()))
            .spawn(move || {
                let _dispatch = tracing::dispatcher::set_default(&dispatch);
                let mut exit = Exit {
                    finished: Some(finished),
                    exited,
                };
                let interrupt = thread_interrupt;
                let watchdog_interrupt = interrupt.clone();
                let watchdog_dispatch = dispatch.clone();
                let watchdog = thread::Builder::new()
                    .name(format!("lucent-js-watchdog-{}", link.id()))
                    .spawn(move || {
                        let _dispatch = tracing::dispatcher::set_default(&watchdog_dispatch);
                        watchdog_interrupt.watch();
                    });
                let watchdog = match watchdog {
                    Ok(watchdog) => watchdog,
                    Err(error) => {
                        interrupt.shut_down();
                        exit.send(Err(error.into()));
                        return;
                    }
                };
                let result = (|| {
                    link.set_waker(Waker::from(wake.clone()));
                    tracing::debug!("script worker booting");
                    let engine = Engine::new(link, interrupt.clone(), io_wake)?;
                    run(
                        engine,
                        execution,
                        &incoming,
                        &wake,
                        &interrupt,
                        Ready {
                            sender: ready,
                            notify: ready_notify,
                        },
                    )
                })();
                let result =
                    if player.is_stopped() && !matches!(result, Err(RuntimeError::Killed(_))) {
                        Err(RuntimeError::Stopped)
                    } else {
                        result
                    };
                exit.send(result);
                interrupt.shut_down();
                let _ = watchdog.join();
            })?;
        let runtime = Self {
            commands,
            interrupt,
            thread: Some(thread),
            terminal,
            ready: boot,
        };
        Ok(runtime)
    }

    fn wait_ready(&self) -> Result<(), RuntimeError> {
        self.ready.recv().map_err(|_| {
            self.terminal
                .recv()
                .unwrap_or(Err(RuntimeError::Stopped))
                .err()
                .unwrap_or(RuntimeError::Stopped)
        })
    }

    pub fn eval(&self, name: impl Into<String>, source: impl Into<String>) -> PendingEval {
        let (reply, pending) = mpsc::channel();
        if self.interrupt.stop().is_some() {
            let _ = reply.send(Err(RuntimeError::Stopped));
            return PendingEval(pending);
        }
        let _ = self.commands.send(Command::Eval {
            name: name.into(),
            source: source.into(),
            reply,
        });
        PendingEval(pending)
    }

    pub fn kill(&self) {
        self.interrupt.kill(Kill::Requested);
        let _ = self.commands.send(Command::Kill);
    }

    #[cfg(feature = "testing")]
    pub fn join_cpu_time(&mut self) -> Option<Duration> {
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
        self.interrupt.killed_turn_cpu_time()
    }
}

struct Exit {
    finished: Option<mpsc::Sender<Result<(), RuntimeError>>>,
    exited: Waker,
}
impl Exit {
    fn send(&mut self, result: Result<(), RuntimeError>) {
        if let Some(finished) = self.finished.take() {
            let _ = finished.send(result);
        }
    }
}
impl Drop for Exit {
    fn drop(&mut self) {
        self.finished = None;
        self.exited.wake_by_ref();
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.interrupt.shut_down();
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            tracing::debug!("script worker joining");
            let _ = thread.join();
        }
    }
}

struct Ready {
    sender: mpsc::Sender<()>,
    notify: Waker,
}

impl Ready {
    fn send(self) {
        let _ = self.sender.send(());
        self.notify.wake();
    }
}

fn run(
    mut engine: Engine,
    mut execution: Execution,
    incoming: &mpsc::Receiver<Command>,
    wake: &EventWake,
    interrupt: &Arc<Interrupt>,
    ready: Ready,
) -> Result<(), RuntimeError> {
    if matches!(execution, Execution::StartScript { .. }) {
        interrupt.prepare_script();
    }
    let mut command = Ok(Command::Wake);
    let mut ready = Some(ready);
    loop {
        interrupt.begin_turn();
        let items = match command {
            Ok(Command::Io) => engine.take_io(),
            Ok(Command::Eval {
                name,
                source,
                reply,
            }) => {
                engine.eval(&name, &source, reply);
                Vec::new()
            }
            Ok(Command::StartScript) => {
                let Execution::StartScript {
                    script,
                    inputs,
                    reply,
                } = execution
                else {
                    unreachable!()
                };
                execution = Execution::Script;
                engine.start(script, inputs, reply);
                Vec::new()
            }
            Ok(Command::StopScript) => {
                if let Execution::StartScript { reply, .. } = execution {
                    let _ = reply.send(Ok(r#"{"kind":"externally-stopped","reason":null}"#.into()));
                    return Ok(());
                }
                engine.stop(None);
                Vec::new()
            }
            Ok(Command::Wake) => {
                wake.queued.store(false, Ordering::Release);
                let Some(items) = take_events(&mut engine) else {
                    return Ok(());
                };
                items
            }
            Ok(Command::Kill) => {
                interrupt.kill(Kill::Requested);
                Vec::new()
            }
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };
        let turn = engine.turn(&serde_json::to_string(&items).unwrap());
        match interrupt.end_turn() {
            None => {}
            Some(Stop::Kill(kill)) => {
                tracing::warn!(%kill, "script runtime killed");
                engine.kill(kill);
                while let Ok(command) = incoming.try_recv() {
                    if let Command::Eval { reply, .. } = command {
                        let _ = reply.send(Err(RuntimeError::Killed(kill)));
                    }
                }
                return Err(RuntimeError::Killed(kill));
            }
            Some(Stop::Shutdown) => return Ok(()),
        }
        let deadline = turn?;
        if let Some(ready) = ready.take() {
            ready.send();
            if matches!(execution, Execution::StartScript { .. }) {
                let _ = wake.commands.send(Command::StartScript);
            }
        }

        if matches!(execution, Execution::Script) && engine.is_idle() {
            return Ok(());
        }
        command = match deadline {
            Some(deadline) => {
                incoming.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            }
            None => incoming.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
    }
}

fn take_events(engine: &mut Engine) -> Option<Vec<host::Item>> {
    collect_events(|| {
        let event = engine.try_event();
        if matches!(event, Err(EventError::Overflow)) {
            engine.events_lost();
        }
        event
    })
}

fn collect_events(
    mut next: impl FnMut() -> Result<Option<crate::bridge::ScriptEvent>, EventError>,
) -> Option<Vec<host::Item>> {
    let mut items = Vec::new();
    loop {
        match next() {
            Ok(Some(event)) => items.push(host::Item::from(event)),
            Ok(None) => return Some(items),
            Err(EventError::Overflow) => {
                items.push(host::Item::EventsLost);
            }
            Err(_) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{Event, ScriptEvent, Value};
    use lucent_protocol::{event::ProjectionEvent, operation::OperationAnswer};
    use serde_json::json;

    #[test]
    fn overflow_preserves_answers_and_zones_already_read_in_the_batch() {
        let mut events = [
            Ok(Some(ScriptEvent::Operation {
                id: 42,
                answer: Ok(OperationAnswer::AcceptDrop(true)),
            })),
            Ok(Some(ScriptEvent::Loader {
                event: Event {
                    name: "onExtensionResponse".into(),
                    args: vec![Value::String("zone".into())],
                },
                projected: vec![ProjectionEvent::Zone {
                    map: "ledgermayne".into(),
                    zone: "A".into(),
                }],
            })),
            Err(EventError::Overflow),
            Ok(Some(ScriptEvent::Invalidated)),
            Ok(None),
        ]
        .into_iter();
        let items = collect_events(|| events.next().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(items).unwrap(),
            json!([
                {"type":"operation","id":"42","answer":{"kind":"acceptDrop","value":true},"error":null},
                {"type":"loader","name":"onExtensionResponse","args":["zone"],"projected":[{"type":"zone","map":"ledgermayne","zone":"A"}]},
                {"type":"eventslost"},
                {"type":"invalidated"}
            ])
        );
    }
}
