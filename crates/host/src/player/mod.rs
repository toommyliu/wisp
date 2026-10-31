mod backends;
mod combat;
mod commands;
pub(crate) mod frames;
mod net;
mod projection;
pub(crate) mod surface;
pub(crate) mod view;

use lucent_protocol::auto_zone::{AutoZoneConfig, AutoZonePatch};
use lucent_protocol::{command::Command, settings::GameSettings};
use parking_lot::RwLock;
use std::collections::HashSet;
use std::ops::ControlFlow;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use ruffle_core::{FloatDuration, Player, PlayerBuilder, config::Letterbox, tag_utils::SwfMovie};

pub(crate) use backends::{Audio, Clipboard, audio};
pub(crate) use surface::Surface;
pub(crate) use view::{ViewEvent, Viewport};

use crate::bridge::{
    Bridge, Call, Error as BridgeError, Events, PendingCall, PendingRead, ScriptEventSender, Value,
};
use crate::projection::Projection;
use crate::sessions::AttachmentId;
use backends::{Navigator, Tasks, Ui};
use net::Net;
use view::Effect;

const STOP_TIMEOUT: Duration = Duration::from_secs(2);
const MIN_INBOX_DRAIN: Duration = Duration::from_millis(2);
const LOADER_SWF: &[u8] = include_bytes!("../../../../assets/loader.swf");

#[derive(Clone)]
pub(crate) struct Movie(pub(crate) Arc<[u8]>);

impl Movie {
    pub(crate) fn loader() -> Self {
        static LOADER: LazyLock<Movie> = LazyLock::new(|| Movie(Arc::from(LOADER_SWF)));
        LOADER.clone()
    }
}

pub(crate) struct Options {
    pub(crate) movie: Movie,
    pub(crate) clipboard: Clipboard,
    pub(crate) audio: Audio,
    pub(crate) muted: bool,
    pub(crate) settings: Arc<RwLock<GameSettings>>,
    pub(crate) auto_zone: Arc<RwLock<AutoZoneConfig>>,
}

pub(crate) struct Handle {
    pub(crate) inbox: Inbox,
    pub(crate) projection: Arc<RwLock<Projection>>,
    pub(crate) stopped: Arc<AtomicBool>,
    pub(crate) operation_packets: Arc<crate::sessions::operations::Packets>,
    exited: mpsc::Receiver<Result<(), PlayerError>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Handle {
    pub(crate) fn send(&self, event: ViewEvent) {
        self.inbox.post(Msg::View(event));
    }

    pub(crate) fn set_muted(&self, muted: bool) {
        self.inbox.post(Msg::Mute(muted));
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.operation_packets.stop();
        self.inbox.post(Msg::Stop);
        match self.exited.recv_timeout(STOP_TIMEOUT) {
            Ok(result) => {
                if let Err(error) = result {
                    tracing::error!(%error, "player stopped");
                }
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
            }
            Err(error) => tracing::error!(%error, "player did not stop cleanly within two seconds"),
        }
    }
}

pub(crate) fn spawn(
    name: String,
    surface: Surface,
    net: tokio::runtime::Handle,
    options: Options,
) -> std::io::Result<Handle> {
    let (tx, rx) = mpsc::channel();
    let inbox = Inbox(tx);
    let (exit_tx, exited) = mpsc::sync_channel(1);
    let outbox = inbox.clone();
    let (provider, loader_events) = Bridge::new();
    let projection = Arc::new(RwLock::new(Projection::default()));
    let stopped = Arc::new(AtomicBool::new(false));
    let thread_stopped = stopped.clone();
    let operation_packets = Arc::new(crate::sessions::operations::Packets::default());
    let thread_packets = operation_packets.clone();
    let bridge = PlayerBridge {
        projection: projection.clone(),
        provider,
        operation_packets: operation_packets.clone(),
        loader_events,
    };
    let thread = thread::Builder::new().name(name).spawn(move || {
        let _stopped = StopOnDrop(thread_stopped, thread_packets);
        let result = run(rx, outbox, surface, net, options, bridge);
        if let Err(error) = &result {
            tracing::error!(%error, "player failed");
        }
        let _ = exit_tx.send(result);
    })?;
    Ok(Handle {
        inbox,
        projection,
        stopped,
        operation_packets,
        exited,
        thread: Some(thread),
    })
}

struct StopOnDrop(Arc<AtomicBool>, Arc<crate::sessions::operations::Packets>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
        self.1.stop();
    }
}

type ReadCallback = Box<dyn FnOnce(Result<&Projection, BridgeError>) + Send>;

pub(crate) struct Read(ReadCallback);

impl Read {
    fn new<R: serde::Serialize + Send + 'static>(
        read: impl FnOnce(&Projection) -> R + Send + 'static,
    ) -> (Read, PendingRead<R>) {
        let (tx, pending) = PendingRead::channel();
        (
            Read(Box::new(move |state| {
                let result = state.and_then(|state| {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(state)))
                        .map_err(|_| BridgeError::ReadPanicked)
                });
                let _ = tx.send(result);
            })),
            pending,
        )
    }
}

pub(crate) enum Msg {
    ObserveProfile {
        priorities: Arc<[lucent_protocol::query::MonsterSelector]>,
        clock: crate::game::jobs::JobClock,
        reply: crate::bridge::Reply<crate::game::jobs::Observation>,
        #[cfg(feature = "testing")]
        probe: Option<crate::game::jobs::ObservationProbe>,
    },
    DispatchProfile(
        crate::game::jobs::Dispatch,
        crate::bridge::Reply<crate::game::jobs::DispatchReply>,
    ),
    PrepareProfile {
        consumable: Option<String>,
        reply: crate::bridge::Reply<crate::game::jobs::prepare::ConsumableInfo>,
    },
    StopProfile(crate::sessions::claims::Binding),
    Read(Read),
    Command(
        lucent_protocol::command::Command,
        crate::bridge::Reply<String>,
    ),
    Call(Call),
    Attach(AttachmentId, ScriptEventSender),
    Detach(AttachmentId),
    #[cfg(feature = "testing")]
    LinkCount(crate::bridge::Reply<usize>),
    OperationAnswer(AttachmentId, crate::bridge::ScriptEvent),
    View(ViewEvent),
    Mute(bool),
    Poll(async_task::Runnable),
    Wake,
    Stop,
}

#[derive(Clone)]
pub(crate) struct Inbox(mpsc::Sender<Msg>);

impl Inbox {
    pub(crate) fn call_loader(&self, name: impl Into<String>, args: Vec<Value>) -> PendingCall {
        let (call, pending) = Call::new(name.into(), args);
        if let Err(error) = self.0.send(Msg::Call(call))
            && let Msg::Call(call) = error.0
        {
            call.reject(BridgeError::Stopped);
        }
        pending
    }

    pub(crate) fn read_projection<R: serde::Serialize + Send + 'static>(
        &self,
        read: impl FnOnce(&Projection) -> R + Send + 'static,
    ) -> PendingRead<R> {
        let (read, pending) = Read::new(read);
        self.post(Msg::Read(read));
        pending
    }

    pub(crate) fn post(&self, msg: Msg) {
        let _ = self.0.send(msg);
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PlayerError {
    #[error("couldn't create the renderer: {0}")]
    Renderer(String),
    #[error("couldn't parse the loader: {0}")]
    Loader(String),
    #[error("couldn't start the network backend: {0}")]
    Net(#[from] net::NetError),
}

fn run(
    inbox: mpsc::Receiver<Msg>,
    outbox: Inbox,
    surface: Surface,
    net: tokio::runtime::Handle,
    options: Options,
    bridge: PlayerBridge,
) -> Result<(), PlayerError> {
    let Some((viewport, muted, script_events)) =
        first_viewport(&inbox, options.muted, &options.settings, &options.auto_zone)
    else {
        return Ok(());
    };
    let mut running = Running::build(viewport, &surface, Net::new(net)?, outbox, options, bridge)?;
    running.script_events = script_events;
    running.set_muted(muted);
    let mut wait = Duration::ZERO;
    'running: loop {
        let frame_deadline = Instant::now() + wait;
        match inbox.recv_timeout(wait) {
            Ok(msg) => {
                if running.handle(msg).is_break() {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let drain_deadline = frame_deadline.max(Instant::now() + MIN_INBOX_DRAIN);
        while Instant::now() < drain_deadline {
            let Ok(msg) = inbox.try_recv() else {
                break;
            };
            if running.handle(msg).is_break() {
                break 'running;
            }
        }
        wait = running.advance();
    }
    running.tasks.borrow_mut().clear();
    while let Ok(msg) = inbox.try_recv() {
        if let Msg::Poll(task) = msg {
            task.run();
        }
    }
    Ok(())
}

fn patch_auto_zone(
    record: &RwLock<AutoZoneConfig>,
    patch: AutoZonePatch,
    links: &ScriptAttachments,
) -> String {
    let mut record = record.write();
    let next = AutoZoneConfig {
        enabled: patch.enabled.unwrap_or(record.enabled),
        map: patch.map.unwrap_or(record.map),
    };
    if *record != next {
        *record = next.clone();
        for (_, stream) in links {
            stream.send(crate::bridge::ScriptEvent::AutoZoneChanged {
                config: next.clone(),
            });
        }
    }
    serde_json::to_string(&*record).unwrap()
}

type ScriptAttachments = Vec<(AttachmentId, ScriptEventSender)>;

fn deliver_operation_answer(
    script_events: &ScriptAttachments,
    attachment: AttachmentId,
    answer: crate::bridge::ScriptEvent,
) {
    if let Some((_, stream)) = script_events.iter().find(|(id, _)| *id == attachment) {
        stream.send(answer);
    }
}

fn first_viewport(
    inbox: &mpsc::Receiver<Msg>,
    mut muted: bool,
    settings: &RwLock<GameSettings>,
    auto_zone: &RwLock<AutoZoneConfig>,
) -> Option<(Viewport, bool, ScriptAttachments)> {
    let mut script_events = Vec::new();
    loop {
        match inbox.recv().ok()? {
            Msg::View(ViewEvent::Resized(viewport)) => {
                return Some((viewport, muted, script_events));
            }
            Msg::Mute(mute) => muted = mute,
            Msg::Attach(id, producer) => {
                producer.send(crate::bridge::ScriptEvent::AutoZoneChanged {
                    config: auto_zone.read().clone(),
                });
                script_events.push((id, producer));
            }
            Msg::Detach(id) => script_events.retain(|(attached, _)| *attached != id),
            #[cfg(feature = "testing")]
            Msg::LinkCount(reply) => {
                let _ = reply.send(Ok(script_events.len()));
            }
            Msg::ObserveProfile { reply, .. } => {
                let _ = reply.send(Err(BridgeError::NotReady));
            }
            Msg::DispatchProfile(_, reply) => {
                let _ = reply.send(Err(BridgeError::NotReady));
            }
            Msg::PrepareProfile { reply, .. } => {
                let _ = reply.send(Err(BridgeError::NotReady));
            }
            Msg::StopProfile(_) => {}
            Msg::Call(call) => call.reject(BridgeError::NotReady),
            Msg::Read(read) => (read.0)(Err(BridgeError::NotReady)),
            Msg::Command(command, reply) => {
                let result = match command {
                    Command::PatchAutoZone { patch } => {
                        Ok(patch_auto_zone(auto_zone, patch, &script_events))
                    }
                    Command::UpdateSettings { mut patch } => {
                        let mut state = settings.write();
                        *state = commands::patched_settings(&state, &mut patch);
                        Ok(serde_json::to_string(&*state).unwrap())
                    }
                    _ => Err(BridgeError::NotReady),
                };
                let _ = reply.send(result);
            }
            Msg::OperationAnswer(attachment, answer) => {
                deliver_operation_answer(&script_events, attachment, answer);
            }
            Msg::Stop => return None,
            Msg::View(_) | Msg::Poll(_) | Msg::Wake => {}
        }
    }
}

struct PlayerBridge {
    operation_packets: Arc<crate::sessions::operations::Packets>,
    projection: Arc<RwLock<Projection>>,
    provider: Bridge,
    loader_events: Events,
}

struct Running {
    settings: Arc<RwLock<GameSettings>>,
    auto_zone: Arc<RwLock<AutoZoneConfig>>,
    settings_applied: bool,
    rejected_settings: HashSet<&'static str>,
    operation_packets: Arc<crate::sessions::operations::Packets>,
    loader_events: Events,
    script_events: ScriptAttachments,
    projection: Arc<RwLock<Projection>>,
    player: Arc<Mutex<Player>>,
    tasks: Tasks,
    viewport: Viewport,
    profile_area: u64,
    last_tick: Instant,
    effects: Vec<Effect>,
    drawn: u32,
    stats_since: Instant,
    surface: Surface,
}

impl Running {
    fn build(
        viewport: Viewport,
        surface: &Surface,
        net: Net,
        inbox: Inbox,
        options: Options,
        bridge: PlayerBridge,
    ) -> Result<Running, PlayerError> {
        let PlayerBridge {
            operation_packets,
            projection,
            provider,
            loader_events,
        } = bridge;
        let renderer = surface.renderer(viewport)?;
        let movie = SwfMovie::from_data(
            &options.movie.0,
            "file:///lucent/loader.swf".into(),
            None,
            None,
        )
        .map_err(|error| PlayerError::Loader(error.to_string()))?;
        let tasks = Tasks::default();
        let player = PlayerBuilder::new()
            .with_boxed_renderer(renderer)
            .with_navigator(Navigator::new(net, inbox, tasks.clone()))
            .with_boxed_audio((options.audio)())
            .with_ui(Ui::new(options.clipboard))
            .with_external_interface(Box::new(provider))
            .with_viewport_dimensions(viewport.width(), viewport.height(), viewport.scale())
            .with_autoplay(true)
            .with_letterbox(Letterbox::On)
            .with_movie(movie)
            .build();
        let now = Instant::now();
        Ok(Running {
            settings: options.settings,
            auto_zone: options.auto_zone,
            settings_applied: false,
            rejected_settings: HashSet::new(),
            operation_packets,
            loader_events,
            script_events: Vec::new(),
            projection,
            player,
            tasks,
            viewport,
            profile_area: 0,
            last_tick: now,
            effects: Vec::new(),
            drawn: 0,
            stats_since: now,
            surface: surface.share(),
        })
    }

    fn handle(&mut self, msg: Msg) -> ControlFlow<()> {
        self.drain_events();
        match msg {
            Msg::Stop => return ControlFlow::Break(()),
            Msg::Attach(id, producer) => {
                producer.send(crate::bridge::ScriptEvent::AutoZoneChanged {
                    config: self.auto_zone.read().clone(),
                });
                self.script_events.push((id, producer));
            }
            Msg::Detach(id) => self.script_events.retain(|(attached, _)| *attached != id),
            #[cfg(feature = "testing")]
            Msg::LinkCount(reply) => {
                let _ = reply.send(Ok(self.script_events.len()));
            }
            Msg::OperationAnswer(attachment, answer) => {
                deliver_operation_answer(&self.script_events, attachment, answer);
            }
            Msg::Read(read) => (read.0)(Ok(&self.projection.read())),
            Msg::ObserveProfile {
                priorities,
                clock,
                reply,
                #[cfg(feature = "testing")]
                probe,
            } => {
                #[cfg(feature = "testing")]
                let began = Instant::now();
                let result = self.observe_profile(priorities, clock);
                #[cfg(feature = "testing")]
                if let Some(probe) = probe {
                    probe(began.elapsed());
                }
                let _ = reply.send(result);
            }
            Msg::DispatchProfile(request, reply) => {
                #[cfg(feature = "testing")]
                let probe = request.probe.clone();
                let result = self.dispatch_profile(request);
                #[cfg(feature = "testing")]
                if let Some(probe) = probe {
                    probe(matches!(
                        result,
                        Ok(crate::game::jobs::DispatchReply::StaleTarget)
                    ));
                }
                let _ = reply.send(result);
            }
            Msg::PrepareProfile { consumable, reply } => {
                let result = self.prepare_profile(consumable);
                let _ = reply.send(result);
            }
            Msg::StopProfile(binding) => self.stop_profile(binding),
            Msg::Command(command, reply) => {
                let result = self.command(command);
                let _ = reply.send(result);
            }
            Msg::Mute(muted) => self.set_muted(muted),
            Msg::Call(mut call) => {
                let result = call.dispatch(&mut self.player.lock().unwrap());
                self.drain_events();
                call.resolve(result);
            }
            Msg::Poll(task) => {
                task.run();
            }
            Msg::Wake => {}
            Msg::View(event) => {
                view::translate(event, self.viewport, &mut self.effects);
                let mut effects = std::mem::take(&mut self.effects);
                for effect in effects.drain(..) {
                    let mut player = self.player.lock().unwrap();
                    match effect {
                        Effect::Event(event) => {
                            player.handle_event(event);
                        }
                        Effect::MouseInStage(inside) => player.set_mouse_in_stage(inside),
                        Effect::Resize(viewport) => {
                            if viewport != self.viewport {
                                player.set_viewport_dimensions(viewport.dimensions());
                                self.viewport = viewport;
                            }
                        }
                    }
                    drop(player);
                    self.drain_events();
                }
                self.effects = effects;
            }
        }
        self.drain_events();
        ControlFlow::Continue(())
    }

    fn set_muted(&self, muted: bool) {
        self.player
            .lock()
            .unwrap()
            .set_volume(if muted { 0.0 } else { 1.0 });
    }

    fn advance(&mut self) -> Duration {
        let now = Instant::now();
        let dt = now - self.last_tick;
        self.last_tick = now;
        let mut player = self.player.lock().unwrap();
        let before = player.frames_run();
        player.tick(FloatDuration::from_std(dt));
        drop(player);
        self.drain_events();
        let mut player = self.player.lock().unwrap();
        if player.frames_run() != before && player.needs_render() && self.surface.wants_frame() {
            player.render();
            self.drawn += 1;
        }
        let elapsed = now.duration_since(self.stats_since);
        if elapsed >= Duration::from_secs(1) {
            tracing::debug!(
                "fps={} viewport={}x{}@{}",
                (f64::from(self.drawn) / elapsed.as_secs_f64()).round() as u32,
                self.viewport.width(),
                self.viewport.height(),
                self.viewport.scale()
            );
            self.drawn = 0;
            self.stats_since = now;
        }
        let wait = player.time_til_next_frame();
        drop(player);
        self.drain_events();
        wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> Running {
        let now = Instant::now();
        let (_provider, loader_events) = Bridge::new();
        Running {
            settings: Arc::new(RwLock::new(GameSettings::default())),
            auto_zone: Arc::default(),
            settings_applied: false,
            rejected_settings: HashSet::new(),
            loader_events,
            script_events: Vec::new(),
            operation_packets: Arc::default(),
            projection: Arc::new(RwLock::new(Projection::default())),
            player: PlayerBuilder::new().build(),
            tasks: Tasks::default(),
            viewport: Viewport::new(1024.0, 768.0, 1.0).unwrap(),
            profile_area: 0,
            last_tick: now,
            effects: Vec::new(),
            drawn: 0,
            stats_since: now,
            surface: Surface::textures(surface::noop_gpu(), || {}),
        }
    }

    #[test]
    fn operation_answers_arrive_before_the_first_viewport() {
        let (tx, rx) = mpsc::channel();
        let (producer, events) =
            crate::bridge::script_events(16, crate::bridge::EventInterests::ALL);
        let (second, second_events) =
            crate::bridge::script_events(16, crate::bridge::EventInterests::ALL);
        let answer = crate::bridge::ScriptEvent::Operation {
            id: 42,
            answer: Ok(lucent_protocol::operation::OperationAnswer::AcceptDrop(
                false,
            )),
        };
        tx.send(Msg::Attach(AttachmentId(0), producer)).unwrap();
        tx.send(Msg::Attach(AttachmentId(1), second)).unwrap();
        tx.send(Msg::OperationAnswer(AttachmentId(0), answer.clone()))
            .unwrap();
        tx.send(Msg::View(ViewEvent::Resized(
            Viewport::new(100.0, 100.0, 1.0).unwrap(),
        )))
        .unwrap();
        let _attachment =
            first_viewport(&rx, false, &Default::default(), &Default::default()).unwrap();
        for stream in [&events, &second_events] {
            assert_eq!(
                stream.try_recv(),
                Ok(Some(crate::bridge::ScriptEvent::AutoZoneChanged {
                    config: Default::default()
                }))
            );
        }
        assert_eq!(events.try_recv(), Ok(Some(answer)));
        assert_eq!(second_events.try_recv(), Ok(None));
    }

    #[test]
    fn operation_answers_for_detached_links_are_discarded() {
        for before_viewport in [true, false] {
            let (producer, events) =
                crate::bridge::script_events(16, crate::bridge::EventInterests::ALL);
            let (second, second_events) =
                crate::bridge::script_events(16, crate::bridge::EventInterests::ALL);
            let messages = [
                Msg::Attach(AttachmentId(0), producer),
                Msg::Attach(AttachmentId(1), second),
                Msg::Detach(AttachmentId(0)),
                Msg::OperationAnswer(
                    AttachmentId(0),
                    crate::bridge::ScriptEvent::Operation {
                        id: 42,
                        answer: Ok(lucent_protocol::operation::OperationAnswer::AcceptDrop(
                            false,
                        )),
                    },
                ),
            ];
            let mut player = (!before_viewport).then(running);
            let _attachments;
            if before_viewport {
                let (tx, rx) = mpsc::channel();
                for message in messages {
                    tx.send(message).unwrap();
                }
                tx.send(Msg::View(ViewEvent::Resized(
                    Viewport::new(100.0, 100.0, 1.0).unwrap(),
                )))
                .unwrap();
                _attachments = Some(
                    first_viewport(&rx, false, &Default::default(), &Default::default()).unwrap(),
                );
            } else {
                _attachments = None;
                let running = player.as_mut().unwrap();
                for message in messages {
                    assert!(running.handle(message).is_continue());
                }
            }
            for stream in [&events, &second_events] {
                assert_eq!(
                    stream.try_recv(),
                    Ok(Some(crate::bridge::ScriptEvent::AutoZoneChanged {
                        config: Default::default()
                    }))
                );
            }
            assert_eq!(second_events.try_recv(), Ok(None));
            assert_eq!(events.try_recv(), Err(BridgeError::Stopped));
        }
    }

    #[test]
    fn attachments_before_the_viewport_are_kept_until_detached_or_stopped() {
        let viewport = Viewport::new(1024.0, 768.0, 1.0).unwrap();
        let (first, first_events) =
            crate::bridge::script_events(1, crate::bridge::EventInterests::ALL);
        let (removed, removed_events) =
            crate::bridge::script_events(1, crate::bridge::EventInterests::ALL);
        let (last, last_events) =
            crate::bridge::script_events(1, crate::bridge::EventInterests::ALL);
        let (tx, rx) = mpsc::channel();
        tx.send(Msg::Attach(AttachmentId(0), first)).unwrap();
        tx.send(Msg::Attach(AttachmentId(1), removed)).unwrap();
        tx.send(Msg::Attach(AttachmentId(2), last)).unwrap();
        tx.send(Msg::Detach(AttachmentId(1))).unwrap();
        tx.send(Msg::View(ViewEvent::Resized(viewport))).unwrap();
        let (received, muted, producers) =
            first_viewport(&rx, false, &Default::default(), &Default::default()).unwrap();
        assert_eq!((received, muted), (viewport, false));
        assert_eq!(producers.len(), 2);
        for stream in [&first_events, &removed_events, &last_events] {
            assert_eq!(
                stream.try_recv(),
                Ok(Some(crate::bridge::ScriptEvent::AutoZoneChanged {
                    config: Default::default()
                }))
            );
        }
        assert_eq!(removed_events.try_recv(), Err(BridgeError::Stopped));
        for (_, producer) in &producers {
            producer.send(crate::bridge::ScriptEvent::Invalidated);
        }
        for events in [&first_events, &last_events] {
            assert_eq!(
                events.try_recv(),
                Ok(Some(crate::bridge::ScriptEvent::Invalidated))
            );
        }
        drop(producers);
        assert_eq!(first_events.try_recv(), Err(BridgeError::Stopped));
        assert_eq!(last_events.try_recv(), Err(BridgeError::Stopped));
    }

    #[test]
    fn stopping_before_the_viewport_ends_an_attachment() {
        let (producer, events) =
            crate::bridge::script_events(1, crate::bridge::EventInterests::ALL);
        let (tx, rx) = mpsc::channel();
        tx.send(Msg::Stop).unwrap();
        tx.send(Msg::Attach(AttachmentId(0), producer)).unwrap();
        assert!(first_viewport(&rx, false, &Default::default(), &Default::default()).is_none());
        drop(rx);
        assert_eq!(events.try_recv(), Err(BridgeError::Stopped));
    }

    #[test]
    fn calls_before_the_first_viewport_are_rejected() {
        let (tx, rx) = mpsc::channel();
        let (call, pending) = Call::new("player.rest".into(), vec![]);
        tx.send(Msg::Call(call)).unwrap();
        tx.send(Msg::Stop).unwrap();
        assert!(first_viewport(&rx, false, &Default::default(), &Default::default()).is_none());
        assert_eq!(
            pending.recv_timeout(Duration::ZERO),
            Err(BridgeError::NotReady)
        );
    }

    #[test]
    fn closing_the_player_inbox_resolves_queued_calls() {
        let (tx, rx) = mpsc::channel();
        let (call, pending) = Call::new("player.rest".into(), vec![]);
        tx.send(Msg::Call(call)).unwrap();
        drop(rx);
        assert_eq!(
            pending.recv_timeout(Duration::ZERO),
            Err(BridgeError::Stopped)
        );
    }

    #[test]
    fn muting_silences_a_player_until_it_is_unmuted() {
        let mut running = running();
        let _ = running.handle(Msg::Mute(true));
        assert_eq!(running.player.lock().unwrap().volume(), 0.0);
        let _ = running.handle(Msg::Mute(false));
        assert_eq!(running.player.lock().unwrap().volume(), 1.0);
    }

    #[test]
    fn a_mute_sent_before_the_first_viewport_is_kept() {
        let viewport = Viewport::new(1024.0, 768.0, 1.0).unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(Msg::Mute(true)).unwrap();
        tx.send(Msg::View(ViewEvent::Resized(viewport))).unwrap();
        assert_eq!(
            first_viewport(&rx, false, &Default::default(), &Default::default())
                .map(|(viewport, muted, _)| (viewport, muted)),
            Some((viewport, true))
        );
    }

    #[test]
    fn a_player_spawned_for_a_muted_session_starts_muted() {
        let viewport = Viewport::new(1024.0, 768.0, 1.0).unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(Msg::View(ViewEvent::Resized(viewport))).unwrap();
        assert_eq!(
            first_viewport(&rx, true, &Default::default(), &Default::default())
                .map(|(viewport, muted, _)| (viewport, muted)),
            Some((viewport, true))
        );
    }
}
