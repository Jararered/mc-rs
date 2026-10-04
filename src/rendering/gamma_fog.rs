//! Shared shader module that blends distance fog in gamma space like Beta's
//! fixed-function pipeline. Block and creature fragment shaders import it.

use bevy::asset::load_internal_asset;
use bevy::asset::uuid_handle;
use bevy::prelude::*;
use bevy::shader::Shader;

const GAMMA_FOG_HANDLE: Handle<Shader> = uuid_handle!("6f2b8f0e-6a52-4c0e-9d0b-3a5d1c7e4b21");

/// Registers `game::gamma_fog`. Safe to call from every plugin that imports it.
pub(crate) fn register(app: &mut App) {
    if !app.world().contains_resource::<Assets<Shader>>() {
        app.init_asset::<Shader>();
    }
    load_internal_asset!(app, GAMMA_FOG_HANDLE, "gamma_fog.wgsl", Shader::from_wgsl);
}
