//! What a click does: mine, break, place, attack, and use an item.
//!
//! `interact_blocks` reads the mouse and writes a `PlayerAction`.
//! `apply_player_actions` applies it to the named player and reads no
//! window, camera, or input.

pub mod attack;
pub mod editing;
pub mod mining;
pub mod overlay;
pub mod use_item;
