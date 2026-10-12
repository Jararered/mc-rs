//! Sound and music, played through Bevy's audio from the player's own copy
//! of Beta's `resources` folder ([`library`]).
//!
//! The game asks for a sound by name through [`Sounds`], wherever in the
//! simulation Beta calls `playSoundEffect`; [`playback`] starts it and sets
//! how loud it is in each ear. [`world`] adds the sounds Beta's client works
//! out for itself, and [`music`] is the background music.

pub mod library;
pub mod music;
pub mod playback;
pub mod world;

use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::audio::AddAudioSource;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::world::sound::Sounds;

/// A seed that differs from run to run, as `new Random()` does.
fn time_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64)
}

/// Plays what the game asks for through [`Sounds`]. Needs Bevy's
/// `AudioPlugin`; without it this adds nothing, `Sounds` does not exist, and
/// every producer stays quiet.
pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::audio::AudioPlugin>() {
            return;
        }
        app.init_resource::<Sounds>()
            .init_resource::<library::SoundLibrary>()
            .init_resource::<music::Music>()
            .init_resource::<world::CaveAmbience>()
            .add_audio_source::<playback::Positioned>()
            .add_systems(Startup, library::start_loading)
            .add_systems(
                Update,
                (
                    library::finish_loading.run_if(library::is_loading),
                    music::play_music,
                    music::apply_music_volume,
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    (
                        world::block_sounds,
                        world::blast_sounds,
                        world::voices,
                        world::cave_ambience,
                        playback::play_sounds,
                    )
                        .chain()
                        .before(TransformSystems::Propagate),
                    (playback::update_emitters, playback::drop_unstarted)
                        .after(TransformSystems::Propagate),
                ),
            );
    }
}
