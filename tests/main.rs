//! The one integration test binary. Each module mirrors the matching
//! `src/<name>/` tree; a second test target would link all of Bevy again.

// `AsBindGroup`'s generated `SystemParam` bound chain through wgpu's internals
// exceeds the default recursion limit of 128.
#![recursion_limit = "256"]

mod app;
mod block;
mod chat;
mod crafting;
mod entity;
mod inventory;
mod item;
mod physics;
mod player;
mod random;
mod rendering;
mod ui;
mod world;
