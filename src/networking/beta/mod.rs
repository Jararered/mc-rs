//! Beta 1.7.3's network protocol (version 14), so the original client can
//! join a world this game is running.
//!
//! `codec` is the wire format, transcribed from the reference `Packet*`
//! classes; `server` is `NetLoginHandler` and `NetServerHandler` over a
//! [`WorldHost`](crate::world::host::WorldHost); `tracker` is
//! `EntityTracker`, which shows clients the entities near them; and
//! `windows` is `Container`, the inventory and container windows and the
//! clicks made in them.

pub mod codec;
mod server;
mod tracker;
mod windows;

pub use server::BetaServer;
pub use server::ServerConfig;

/// `Packet1Login.protocolVersion` for Beta 1.7.3.
pub const PROTOCOL_VERSION: i32 = 14;
