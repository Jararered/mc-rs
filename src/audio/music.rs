//! Background music: `SoundManager.playRandomMusicIfReady`. Beta waits up to
//! ten minutes after starting, plays one track from `music/` and
//! `newmusic/`, and then waits ten to twenty minutes for the next.

use std::path::PathBuf;

use bevy::audio::AudioPlayer;
use bevy::audio::AudioSink;
use bevy::audio::AudioSinkPlayback;
use bevy::audio::AudioSource;
use bevy::audio::PlaybackSettings;
use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy::tasks::Task;
use bevy::tasks::futures::check_ready;

use super::library::SoundLibrary;
use super::library::read_source;
use super::playback::Unstarted;
use super::time_seed;
use crate::app::settings::GameSettings;
use crate::random::JavaRandom;
use crate::world::sound::SoundCategory;
use crate::world::tick::WorldTick;

/// The longest wait before the first track, and the shortest between tracks.
pub const MUSIC_DELAY_TICKS: u32 = 12000;

/// `SoundManager.ticksBeforeMusic` and the `Random` it is drawn from.
#[derive(Debug)]
pub struct MusicTimer {
    ticks_before_music: u32,
    random: JavaRandom,
}

impl MusicTimer {
    pub fn new(seed: u64) -> Self {
        let mut random = JavaRandom::new(seed);
        Self {
            ticks_before_music: random.next_int(MUSIC_DELAY_TICKS),
            random,
        }
    }

    pub fn ticks_before_music(&self) -> u32 {
        self.ticks_before_music
    }

    /// One client tick with nothing playing. Returns whether a track is due,
    /// and if so starts the wait for the one after it.
    pub fn tick(&mut self) -> bool {
        if self.ticks_before_music > 0 {
            self.ticks_before_music -= 1;
            return false;
        }
        self.ticks_before_music = self.random.next_int(MUSIC_DELAY_TICKS) + MUSIC_DELAY_TICKS;
        true
    }
}

#[derive(Resource)]
pub struct Music {
    timer: MusicTimer,
    /// The next track, being read from disk.
    loading: Option<Task<Option<AudioSource>>>,
}

impl Default for Music {
    fn default() -> Self {
        Self {
            timer: MusicTimer::new(time_seed()),
            loading: None,
        }
    }
}

/// The track that is playing: Beta's `BgMusic` source.
#[derive(Component)]
pub struct MusicTrack;

fn read_track(path: PathBuf) -> Option<AudioSource> {
    let source = read_source(&path);
    if source.is_none() {
        warn!(
            "Skipping {}: not a readable Ogg Vorbis file",
            path.display()
        );
    }
    source
}

pub(super) fn play_music(
    mut commands: Commands,
    tick: Res<WorldTick>,
    settings: Option<Res<GameSettings>>,
    mut music: ResMut<Music>,
    mut library: ResMut<SoundLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
    playing: Query<(), With<MusicTrack>>,
) {
    let volume = settings.map_or(1.0, |settings| settings.gain(SoundCategory::Music));
    if volume <= 0.0 {
        return;
    }
    if let Some(track) = music.loading.as_mut().and_then(check_ready) {
        music.loading = None;
        if let Some(track) = track {
            commands.spawn((
                AudioPlayer::new(sources.add(track)),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
                MusicTrack,
                Unstarted::default(),
            ));
            return;
        }
    }
    if music.loading.is_some() || !playing.is_empty() {
        return;
    }
    for _ in 0..tick.ticks_this_frame() {
        if !music.timer.tick() {
            continue;
        }
        if let Some(path) = library.pick_music() {
            music.loading = Some(IoTaskPool::get().spawn(async move { read_track(path) }));
        }
        break;
    }
}

/// `SoundManager.onSoundOptionsChanged`: the Music and Master options reach
/// the track that is playing, and turning either off stops the track.
pub(super) fn apply_music_volume(
    mut commands: Commands,
    settings: Res<GameSettings>,
    mut tracks: Query<(Entity, Option<&mut AudioSink>), With<MusicTrack>>,
) {
    if !settings.is_changed() {
        return;
    }
    let volume = settings.gain(SoundCategory::Music);
    for (entity, sink) in &mut tracks {
        if volume <= 0.0 {
            commands.entity(entity).despawn();
        } else if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(volume));
        }
    }
}
