// `AsBindGroup`'s generated `SystemParam` bound chain through wgpu's internals
// exceeds the default recursion limit of 128.
#![recursion_limit = "256"]

pub mod block;

pub mod app;
pub mod crafting;
pub mod entity;
pub mod inventory;
pub mod item;
pub mod physics;
pub mod player;
pub mod random;
pub mod ui;
pub mod world;
