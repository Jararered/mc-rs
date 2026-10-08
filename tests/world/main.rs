//! Integration tests for the `world` subsystem, mirroring `src/world`.

mod block_ticks;
mod chunk;
mod generation;
mod persistence;
mod streaming;

mod combat;
mod mobs;
mod pathfinding;
mod portal;
mod runtime;
mod sleep;
mod weather;

// Share the world test binary to avoid another full Bevy link.
#[path = "../rendering/mod.rs"]
mod rendering;
