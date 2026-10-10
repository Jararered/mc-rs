//! Beta 1.7.3's network protocol (version 14), so the original client can
//! join a world this game is running.
//!
//! `codec` is the wire format, transcribed from the reference `Packet*`
//! classes; `server` is `NetLoginHandler` and `NetServerHandler` over a
//! [`WorldHost`](crate::world::host::WorldHost).

pub mod codec;
mod server;

pub use server::BetaServer;
pub use server::ServerConfig;

/// `Packet1Login.protocolVersion` for Beta 1.7.3.
pub const PROTOCOL_VERSION: i32 = 14;
