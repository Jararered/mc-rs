//! Client assembly: screens, settings, and the world session.
//!
//! [`GamePlugin`] composes the client. World simulation runs without this
//! module. See `docs/ARCHITECTURE.md`.

mod diagnostics;
pub mod frame_pacing;
pub mod fullscreen;
mod plugin;
pub mod screenshot;
pub mod session;
pub mod settings;
pub mod state;

pub use plugin::GamePlugin;
