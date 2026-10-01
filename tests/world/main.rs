//! Integration tests for the `world` subsystem, mirroring `src/world`.

mod block_ticks;
mod chunk;
mod generation;
mod persistence;
mod streaming;

mod runtime;

// Share the world test binary to avoid another full Bevy link.
#[path = "../rendering/mod.rs"]
mod rendering;
