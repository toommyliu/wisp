mod controller;
mod environment;
mod environment_order;
pub use controller::{ScriptControls, ScriptRequest, Supervisor};
mod reaper;
pub(crate) mod relogin;
pub use reaper::reap;
pub(crate) mod claims;
mod link;
pub use claims::ClaimGrant;
pub(crate) mod operations;
pub use link::{PlayerStatus, ScriptLink};

use lucent_protocol::auto_zone::AutoZoneConfig;
use lucent_protocol::settings::GameSettings;
use parking_lot::RwLock;

pub(crate) struct Records {
    pub(crate) files: Arc<crate::data::Files>,
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
        let player = self.spawn_player(
            id,
            &surface,
            false,
            &movie,
            settings.clone(),
            auto_zone.clone(),
        )?;
        let operations = operations::Operations::start(id).map_err(HostError::OperationThread)?;
        let relogin = Arc::new(relogin::Shared::default());
        Ok(Session {
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
        settings: Arc<RwLock<GameSettings>>,
        auto_zone: Arc<RwLock<AutoZoneConfig>>,
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
                settings,
                auto_zone,
            },
        )
        .map_err(HostError::PlayerThread)
    }
}

pub struct Session {
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
            Records {
                files: self.host.shared.files.clone(),
                jobs: self.jobs.clone(),
                environment: self.environment.clone(),
                settings: self.settings.clone(),
                auto_zone: self.auto_zone.clone(),
                auto_relogin: self.auto_relogin.clone(),
                relogin: self.relogin.clone(),
            },
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
            self.settings.clone(),
            self.auto_zone.clone(),
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
