pub mod automation;
pub mod bridge;
mod data;
pub mod game;
pub mod library;
mod player;
pub mod projection;
mod sessions;

pub use bridge::PendingRead;
pub use player::frames::FrameLease;
pub use player::surface::{Gpu, Surface};
pub use player::view::{
    Ime, Key, KeyCode, Modifiers, MouseButton, NamedKey, Point, ViewEvent, Viewport, WheelDelta,
};
pub use ruffle_render_wgpu::wgpu;
pub use sessions::reap;
pub use sessions::{Host, HostError, PlayerStatus, ScriptLink, Session};

#[cfg(feature = "testing")]
pub mod testing;

pub mod scripting;
pub use sessions::{ScriptControls, ScriptRequest, Supervisor};
