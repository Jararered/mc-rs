//! Records: `RenderGlobal.playRecord` and `SoundManager.playStreaming`. One
//! record plays at a time, from the jukebox it was put in, and it is heard
//! four times as far as an ordinary sound.

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy::tasks::Task;
use bevy::tasks::futures::check_ready;

use super::library::SoundLibrary;
use super::library::read_source;
use super::music::MusicTrack;
use super::playback::HEARING_RANGE;
use super::playback::Positioned;
use super::playback::Start;
use super::playback::listener;
use super::playback::start;
use crate::app::settings::GameSettings;
use crate::item::Item;
use crate::player::PlayerCamera;
use crate::world::block_ticks::AuxEffect;
use crate::world::block_ticks::behaviors::jukebox::RECORD_EFFECT;
use crate::world::sound::SoundCategory;
use crate::world::tick::WorldTick;

/// `playStreaming` plays a record at half volume.
pub const RECORD_VOLUME: f32 = 0.5;

/// How far a record carries.
pub const RECORD_RANGE: f32 = HEARING_RANGE * 4.0;

/// Ticks `GuiIngame` shows the name of a record that has started.
pub const NOW_PLAYING_TICKS: u32 = 60;

/// The record that is playing: Beta's `streaming` source.
#[derive(Component)]
pub struct Record;

/// The record being read from disk, and the jukebox it will play from.
#[derive(Resource, Default)]
pub struct RecordPlayer {
    loading: Option<(Task<Option<AudioSource>>, Vec3)>,
}

/// `GuiIngame.recordPlaying`: the line over the hotbar that names a record
/// as it starts.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct NowPlaying {
    pub text: String,
    /// `recordPlayingUpFor`.
    pub ticks: u32,
}

impl NowPlaying {
    /// `setRecordPlayingMessage`, with `playRecord`'s credit.
    pub fn announce(&mut self, record: &str) {
        self.text = format!("Now playing: C418 - {record}");
        self.ticks = NOW_PLAYING_TICKS;
    }
}

/// Aux effect 1005: a jukebox starts the disc it was given, or with no disc
/// stops the one that is playing. Either way the last record ends, and a
/// new one ends the background music too.
pub(super) fn play_records(
    mut commands: Commands,
    mut effects: MessageReader<AuxEffect>,
    tick: Res<WorldTick>,
    settings: Option<Res<GameSettings>>,
    mut player: ResMut<RecordPlayer>,
    mut now_playing: ResMut<NowPlaying>,
    mut library: ResMut<SoundLibrary>,
    mut playings: ResMut<Assets<Positioned>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    records: Query<Entity, With<Record>>,
    music: Query<Entity, With<MusicTrack>>,
) {
    now_playing.ticks = now_playing.ticks.saturating_sub(tick.ticks_this_frame());
    for effect in effects.read() {
        if effect.effect != RECORD_EFFECT {
            continue;
        }
        for record in &records {
            commands.entity(record).despawn();
        }
        player.loading = None;
        let Some(name) = u16::try_from(effect.data)
            .ok()
            .and_then(Item::from_u16)
            .and_then(Item::record_name)
        else {
            continue;
        };
        now_playing.announce(name);
        let Some(path) = library.pick_record(name) else {
            continue;
        };
        for track in &music {
            commands.entity(track).despawn();
        }
        let jukebox = effect.position.as_vec3();
        player.loading = Some((
            IoTaskPool::get().spawn(async move { read_source(&path) }),
            jukebox,
        ));
    }
    let Some((task, jukebox)) = player.loading.as_mut() else {
        return;
    };
    let Some(source) = check_ready(task) else {
        return;
    };
    let jukebox = *jukebox;
    player.loading = None;
    let Some(source) = source else {
        return;
    };
    let master = settings.map_or(1.0, |settings| settings.gain(SoundCategory::NoteBlocks));
    start(
        &mut commands,
        &mut playings,
        listener(&camera),
        master,
        Start {
            source,
            position: Some(jukebox),
            volume: RECORD_VOLUME,
            range: RECORD_RANGE,
            pitch: 1.0,
            category: SoundCategory::NoteBlocks,
        },
    )
    .insert(Record);
}

/// Whether a record is playing or about to, which holds the music off.
pub(super) fn record_playing(player: &RecordPlayer, records: &Query<(), With<Record>>) -> bool {
    player.loading.is_some() || !records.is_empty()
}
