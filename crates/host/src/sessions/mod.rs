pub(crate) mod auto_attack;
mod changes;
mod controller;
use crate::scripting::RuntimeError;
pub(crate) use changes::Changes;
use lucent_protocol::script::{Script, ScriptStatus};
use std::sync::atomic::AtomicBool;
use std::task::Waker;
mod environment;
mod environment_order;
pub use controller::{ScriptControls, ScriptRequest, Supervisor};
mod reaper;
pub(crate) mod relogin;
pub use reaper::reap;
pub(crate) mod claims;
pub mod lifecycle;
mod link;
pub use claims::ClaimGrant;
pub(crate) mod operations;
pub use link::{PlayerStatus, ScriptLink};

use lucent_protocol::auto_zone::AutoZoneConfig;
use lucent_protocol::settings::GameSettings;
use parking_lot::RwLock;

#[derive(Clone)]
pub(crate) struct Records {
    pub(crate) files: Arc<crate::data::Files>,
    pub(crate) changes: Arc<Changes>,
    pub(crate) auto_attack: Arc<auto_attack::Shared>,
    pub(crate) jobs: Arc<crate::game::jobs::JobRequests>,
    pub(crate) settings: Arc<RwLock<GameSettings>>,
    environment: Arc<RwLock<lucent_protocol::environment::EnvironmentState>>,
    auto_zone: Arc<RwLock<AutoZoneConfig>>,
    pub(crate) auto_relogin: Arc<RwLock<lucent_protocol::auto_relogin::AutoReloginRecords>>,
    pub(crate) relogin: Arc<relogin::Shared>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AttachmentId(pub(crate) u64);

use std::sync::Arc;

use crate::bridge::{EVENT_CAPACITY, PendingRead, script_events};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::player::frames::FrameLease;
use crate::player::{self, Audio, Clipboard, Movie, Surface, ViewEvent};

#[derive(Clone)]
pub struct Host {
    shared: Arc<Shared>,
}

struct Shared {
    files: Arc<crate::data::Files>,
    net: tokio::runtime::Runtime,
    clipboard: Clipboard,
    audio: Audio,
    next_session: AtomicU64,
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("couldn't start the files service: {0}")]
    Files(std::io::Error),
    #[error("couldn't start the net runtime: {0}")]
    NetRuntime(std::io::Error),
    #[error("couldn't start a player thread: {0}")]
    PlayerThread(std::io::Error),
    #[error("couldn't start an operation worker: {0}")]
    OperationThread(std::io::Error),
    #[error("couldn't start the clipboard thread: {0}")]
    ClipboardThread(std::io::Error),
}

impl Host {
    pub fn new() -> Result<Host, HostError> {
        Self::with_services(
            Clipboard::system().map_err(HostError::ClipboardThread)?,
            player::audio,
        )
    }

    pub(crate) fn with_services(clipboard: Clipboard, audio: Audio) -> Result<Host, HostError> {
        let root =
            crate::data::data_root(dirs::document_dir, dirs::home_dir).map_err(HostError::Files)?;
        Self::with_services_and_data_root(clipboard, audio, root)
    }

    pub(crate) fn with_services_and_data_root(
        clipboard: Clipboard,
        audio: Audio,
        root: std::path::PathBuf,
    ) -> Result<Host, HostError> {
        let net = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("lucent-net")
            .enable_all()
            .build()
            .map_err(HostError::NetRuntime)?;
        Ok(Host {
            shared: Arc::new(Shared {
                files: Arc::new(crate::data::Files::start(root).map_err(HostError::Files)?),
                net,
                clipboard,
                audio,
                next_session: AtomicU64::new(1),
            }),
        })
    }

    pub fn start_session(&self, surface: Surface) -> Result<Session, HostError> {
        self.start_movie(surface, Movie::loader())
    }

    pub(crate) fn start_movie(&self, surface: Surface, movie: Movie) -> Result<Session, HostError> {
        let id = self.shared.next_session.fetch_add(1, Ordering::Relaxed);
        let settings = Arc::new(RwLock::new(GameSettings::default()));
        let auto_zone = Arc::new(RwLock::new(AutoZoneConfig::default()));
        let changes = Arc::new(Changes::default());
        let player = self.spawn_player(
            id,
            &surface,
            false,
            &movie,
            PlayerRecords {
                settings: settings.clone(),
                auto_zone: auto_zone.clone(),
                changes: changes.clone(),
            },
        )?;
        let operations = operations::Operations::start(id).map_err(HostError::OperationThread)?;
        let relogin = Arc::new(relogin::Shared::new(changes.clone()));
        let auto_attack = Arc::new(auto_attack::Shared::new(changes.clone()));
        Ok(Session {
            supervisor: None,
            script_status: Arc::default(),
            changes,
            detached: Arc::default(),
            auto_attack,
            jobs: Arc::default(),
            environment: Arc::default(),
            auto_relogin: relogin.records.clone(),
            relogin,
            next_attachment: 0,
            operations,
            player: Some(player),
            movie,
            id,
            surface,
            muted: false,
            auto_zone,
            settings,
            host: self.clone(),
        })
    }

    fn spawn_player(
        &self,
        session: u64,
        surface: &Surface,
        muted: bool,
        movie: &Movie,
        records: PlayerRecords,
    ) -> Result<player::Handle, HostError> {
        player::spawn(
            format!("player-{session}"),
            surface.share(),
            self.shared.net.handle().clone(),
            player::Options {
                clipboard: self.shared.clipboard.clone(),
                audio: self.shared.audio,
                movie: movie.clone(),
                muted,
                settings: records.settings,
                auto_zone: records.auto_zone,
                changes: records.changes,
            },
        )
        .map_err(HostError::PlayerThread)
    }
}

struct PlayerRecords {
    settings: Arc<RwLock<GameSettings>>,
    auto_zone: Arc<RwLock<AutoZoneConfig>>,
    changes: Arc<Changes>,
}

pub struct Session {
    supervisor: Option<Supervisor>,
    script_status: Arc<RwLock<ScriptStatus>>,
    changes: Arc<Changes>,
    detached: Arc<AtomicBool>,
    auto_attack: Arc<auto_attack::Shared>,
    jobs: Arc<crate::game::jobs::JobRequests>,
    environment: Arc<RwLock<lucent_protocol::environment::EnvironmentState>>,
    auto_relogin: Arc<RwLock<lucent_protocol::auto_relogin::AutoReloginRecords>>,
    relogin: Arc<relogin::Shared>,
    player: Option<player::Handle>,
    next_attachment: u64,
    operations: operations::Operations,
    movie: Movie,
    id: u64,
    surface: Surface,
    muted: bool,
    host: Host,
    settings: Arc<RwLock<GameSettings>>,
    auto_zone: Arc<RwLock<AutoZoneConfig>>,
}

impl Session {
    pub fn supervise(&mut self, script: Option<ScriptRequest>) -> Result<(), RuntimeError> {
        if self.supervisor.is_none() {
            self.supervisor = Some(Supervisor::start(self, script)?);
        }
        Ok(())
    }
    pub fn restart_player(&mut self, surface: Surface) -> Result<(), HostError> {
        let supervisor = self.supervisor.take();
        match supervisor {
            Some(mut supervisor) => {
                let result = supervisor.restart_player(self, surface);
                self.supervisor = Some(supervisor);
                result
            }
            None => self.restart(surface),
        }
    }
    pub fn start_script(&mut self, script: Script) -> Result<(), RuntimeError> {
        let mut supervisor = self.supervisor.take().ok_or(RuntimeError::Stopped)?;
        let result = supervisor.start_script(self, script).map(|_| ());
        self.supervisor = Some(supervisor);
        result
    }
    pub fn stop_script(&self) {
        if let Some(supervisor) = &self.supervisor {
            supervisor.controls().stop();
        }
    }
    pub fn kill_script(&self) {
        if let Some(supervisor) = &self.supervisor {
            supervisor.controls().kill();
        }
    }
    pub fn script_status(&self) -> ScriptStatus {
        self.script_status.read().clone()
    }
    pub fn settings(&self) -> GameSettings {
        self.settings.read().clone()
    }
    pub fn auto_zone(&self) -> AutoZoneConfig {
        self.auto_zone.read().clone()
    }
    pub fn watch(&self, waker: Waker) {
        self.changes.watch(waker);
    }
    pub fn command(
        &self,
        command: lucent_protocol::command::Command,
    ) -> crate::bridge::Pending<String> {
        let Some(player) = &self.player else {
            return crate::bridge::Pending::stopped();
        };
        link::route_command(
            &self.records(),
            &player.inbox,
            &player.stopped,
            &self.detached,
            command,
        )
    }
    fn records(&self) -> Records {
        Records {
            files: self.host.shared.files.clone(),
            auto_attack: self.auto_attack.clone(),
            changes: self.changes.clone(),
            jobs: self.jobs.clone(),
            environment: self.environment.clone(),
            settings: self.settings.clone(),
            auto_zone: self.auto_zone.clone(),
            auto_relogin: self.auto_relogin.clone(),
            relogin: self.relogin.clone(),
        }
    }

    #[cfg(feature = "testing")]
    pub fn live_profile_job_count(&self) -> usize {
        self.jobs.len()
    }

    #[cfg(feature = "testing")]
    pub fn set_profile_observation_probe(
        &self,
        probe: impl Fn(std::time::Duration) + Send + Sync + 'static,
    ) {
        *self.jobs.observation_probe.lock() = Some(Arc::new(probe));
    }

    #[cfg(feature = "testing")]
    pub fn set_profile_action_probe(&self, probe: impl Fn(bool) + Send + Sync + 'static) {
        *self.jobs.action_probe.lock() = Some(Arc::new(probe));
    }

    #[cfg(feature = "testing")]
    pub fn set_profile_dispatch_probe(&self, probe: impl Fn(u64, bool) + Send + Sync + 'static) {
        *self.jobs.dispatch_probe.lock() = Some(Arc::new(probe));
    }

    #[cfg(feature = "testing")]
    pub fn attached_link_count(&self) -> crate::bridge::Pending<usize> {
        let (reply, pending) = crate::bridge::Pending::channel();
        if let Some(player) = &self.player {
            player.inbox.post(player::Msg::LinkCount(reply));
        }
        pending
    }

    /// Runs after preceding events and returns owned, serializable data.
    ///
    /// ```
    /// use lucent_host::Session;
    /// use lucent_protocol::item::ItemContainer;
    /// fn snapshot(session: &Session) {
    ///     let _ = session.read_projection(|state| {
    ///         state.items.get(ItemContainer::Inventory, 1).map(|item| item.data().clone())
    ///     });
    /// }
    /// ```
    ///
    /// Live handles cannot cross this boundary.
    ///
    /// ```compile_fail
    /// use lucent_host::Session;
    /// use lucent_protocol::item::ItemContainer;
    /// fn live_handle(session: &Session) {
    ///     let _ = session.read_projection(|state| state.items.get(ItemContainer::Inventory, 1));
    /// }
    /// ```
    pub fn read_projection<R: serde::Serialize + Send + 'static>(
        &self,
        read: impl FnOnce(&crate::projection::Projection) -> R + Send + 'static,
    ) -> PendingRead<R> {
        match &self.player {
            Some(player) => player.inbox.read_projection(read),
            None => PendingRead::stopped(),
        }
    }

    #[cfg(feature = "testing")]
    pub fn pending_relogin_count(&self) -> usize {
        self.relogin.count()
    }

    pub fn auto_attack(&self) -> lucent_protocol::auto_attack::AutoAttackRecords {
        self.auto_attack.records.read().clone()
    }

    pub fn auto_relogin(&self) -> lucent_protocol::auto_relogin::AutoReloginRecords {
        self.auto_relogin.read().clone()
    }

    pub fn attach(&mut self, interests: crate::bridge::EventInterests) -> Option<ScriptLink> {
        let player = self.player.as_ref()?;
        let attachment = AttachmentId(self.next_attachment);
        self.next_attachment = self
            .next_attachment
            .checked_add(1)
            .expect("script attachment IDs exhausted");
        let inbox = player.inbox.clone();
        let (producer, events) = script_events(EVENT_CAPACITY, interests);
        inbox.post(player::Msg::Attach(attachment, producer));
        Some(ScriptLink::new(
            self.id,
            attachment,
            events,
            self.records(),
            operations::Endpoint {
                inbox,
                projection: player.projection.clone(),
                stopped: player.stopped.clone(),
                packets: player.operation_packets.clone(),
            },
            self.operations.sender.clone(),
        ))
    }

    pub fn send(&self, event: ViewEvent) {
        if let Some(player) = &self.player {
            player.send(event);
        }
    }

    pub fn frame(&self) -> Option<FrameLease> {
        self.surface.lease()
    }

    pub fn restart(&mut self, surface: Surface) -> Result<(), HostError> {
        self.player = None;
        self.surface = surface;
        let player = self.host.spawn_player(
            self.id,
            &self.surface,
            self.muted,
            &self.movie,
            PlayerRecords {
                settings: self.settings.clone(),
                auto_zone: self.auto_zone.clone(),
                changes: self.changes.clone(),
            },
        )?;
        self.player = Some(player);
        Ok(())
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        if let Some(player) = &self.player {
            player.set_muted(muted);
        }
    }
}

impl Host {
    pub fn data_root(&self) -> &std::path::Path {
        self.shared.files.root()
    }

    #[cfg(feature = "testing")]
    pub fn set_file_probe(
        &self,
        probe: impl Fn(crate::testing::FileStage, &str) + Send + Sync + 'static,
    ) {
        self.shared.files.set_probe(Arc::new(probe));
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.detached.store(true, Ordering::Release);
    }
}
