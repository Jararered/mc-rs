//! Starting the sounds the game asks for, and keeping each one's loudness and
//! stereo side right while it plays.
//!
//! Beta hands every sound to OpenAL with a position and a linear rolloff: at
//! full volume a sound fades evenly to nothing 16 blocks away. Bevy's spatial
//! audio falls off with the square of the distance instead, so sounds here
//! are played flat and [`Stereo`] sets the gain of each ear, from
//! [`channel_gains`], as the listener moves.

use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bevy::audio::AudioPlayer;
use bevy::audio::AudioSink;
use bevy::audio::AudioSource;
use bevy::audio::ChannelCount;
use bevy::audio::Decodable;
use bevy::audio::PlaybackSettings;
use bevy::audio::Sample;
use bevy::audio::SampleRate;
use bevy::audio::Source;
use bevy::prelude::*;

use super::library::SoundLibrary;
use crate::app::settings::GameSettings;
use crate::player::PlayerCamera;
use crate::world::sound::MAX_QUEUED;
use crate::world::sound::SoundCategory;
use crate::world::sound::Sounds;

/// How far a sound of volume 1 carries: `RenderGlobal.playSound` plays
/// nothing beyond it, and `SoundManager.playSound` fades to silence there.
pub const HEARING_RANGE: f32 = 16.0;

/// `SoundManager.playSoundFX` plays at a quarter of the volume asked for.
pub const UI_VOLUME: f32 = 0.25;

/// paulscode's `Source.setPitch` keeps a pitch within these.
pub const MIN_PITCH: f32 = 0.5;
pub const MAX_PITCH: f32 = 2.0;

/// The share of the way to its target a gain moves each sample, so a turn of
/// the head is a ramp of a few milliseconds and not a click.
const GAIN_EASE: f32 = 0.005;

const STEREO: ChannelCount = ChannelCount::new(2).unwrap();

/// Seconds an [`AudioPlayer`] may wait for Bevy to start it. With no output
/// device Bevy starts nothing, and the entity would otherwise stay for ever.
const START_TIMEOUT: f32 = 1.0;

/// Where the player's ears are. `SoundManager.func_338_a` puts the listener
/// at the player and turns it with their yaw alone, so its right is level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub position: Vec3,
    pub right: Vec3,
}

impl Listener {
    pub fn of(camera: &GlobalTransform) -> Self {
        let right = camera.right();
        Self {
            position: camera.translation(),
            right: Vec3::new(right.x, 0.0, right.z).normalize_or_zero(),
        }
    }
}

/// How far a sound of Beta `volume` carries. A volume above 1 is no louder;
/// it reaches further (an explosion's 4 is heard 64 blocks off).
pub fn hearing_range(volume: f32) -> f32 {
    HEARING_RANGE * volume.max(1.0)
}

/// A sound's gain `distance` blocks from the listener: paulscode's linear
/// attenuation over [`hearing_range`].
pub fn attenuation(distance: f32, volume: f32) -> f32 {
    volume.min(1.0) * (1.0 - distance / hearing_range(volume)).clamp(0.0, 1.0)
}

/// Equal-power stereo: -1 is all left, 0 the middle, 1 all right.
fn pan(side: f32) -> [f32; 2] {
    let angle = (side.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    [angle.cos(), angle.sin()]
}

/// The left and right gain of a sound of Beta `volume`. With no `position`
/// it is `playSoundFX`'s: in the middle, wherever the listener is.
pub fn channel_gains(listener: Option<Listener>, position: Option<Vec3>, volume: f32) -> [f32; 2] {
    let Some(position) = position else {
        return pan(0.0).map(|channel| channel * volume.min(1.0) * UI_VOLUME);
    };
    let Some(listener) = listener else {
        return [0.0; 2];
    };
    let offset = position - listener.position;
    let distance = offset.length();
    // A sound at the listener's own position has no side.
    let side = if distance > 1e-3 {
        offset.dot(listener.right) / distance
    } else {
        0.0
    };
    let gain = attenuation(distance, volume);
    pan(side).map(|channel| channel * gain)
}

/// The two gains of a playing sound, written by the game and read on the
/// audio thread.
#[derive(Debug, Default)]
pub struct ChannelGains {
    left: AtomicU32,
    right: AtomicU32,
}

impl ChannelGains {
    pub fn new(gains: [f32; 2]) -> Self {
        let shared = Self::default();
        shared.set(gains);
        shared
    }

    pub fn set(&self, [left, right]: [f32; 2]) {
        self.left.store(left.to_bits(), Ordering::Relaxed);
        self.right.store(right.to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self) -> [f32; 2] {
        [
            f32::from_bits(self.left.load(Ordering::Relaxed)),
            f32::from_bits(self.right.load(Ordering::Relaxed)),
        ]
    }
}

/// One playing of a sound file: its data and the gains it is heard at.
#[derive(Asset, TypePath, Clone, Debug)]
pub struct Positioned {
    source: AudioSource,
    gains: Arc<ChannelGains>,
}

impl Decodable for Positioned {
    type Decoder = Stereo<<AudioSource as Decodable>::Decoder>;

    fn decoder(&self) -> Self::Decoder {
        Stereo::new(self.source.decoder(), Arc::clone(&self.gains))
    }
}

/// Mixes its input down to one channel and plays it on two, each at its own
/// gain.
pub struct Stereo<I> {
    input: I,
    gains: Arc<ChannelGains>,
    /// The gains in use, eased towards the ones asked for.
    current: [f32; 2],
    /// The right-hand sample of the frame whose left was just returned.
    right: Option<Sample>,
}

impl<I> Stereo<I> {
    pub fn new(input: I, gains: Arc<ChannelGains>) -> Self {
        Self {
            input,
            current: gains.get(),
            gains,
            right: None,
        }
    }
}

impl<I: Source> Iterator for Stereo<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        let channels = self.input.channels().get();
        let mut sum = self.input.next()?;
        for _ in 1..channels {
            sum += self.input.next().unwrap_or(0.0);
        }
        let mono = sum / Sample::from(channels);
        for (current, target) in self.current.iter_mut().zip(self.gains.get()) {
            *current += (target - *current) * GAIN_EASE;
        }
        self.right = Some(mono * self.current[1]);
        Some(mono * self.current[0])
    }
}

impl<I: Source> Source for Stereo<I> {
    // Two channels at the file's rate from start to end.
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        STEREO
    }

    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}

/// A sound playing in the world, or in the player's head for one with no
/// position.
#[derive(Component)]
pub struct Emitter {
    position: Option<Vec3>,
    /// Beta's volume, before distance and the Audio options.
    volume: f32,
    category: SoundCategory,
    gains: Arc<ChannelGains>,
}

/// A sound with a place in the world, which stops when the world is left.
/// One without (`playSoundFX`: a button's click, the portal's rush) plays on.
#[derive(Component)]
pub struct WorldSound;

/// How long an [`AudioPlayer`] has waited to be started.
#[derive(Component, Default)]
pub struct Unstarted(pub f32);

fn listener(camera: &Query<&GlobalTransform, With<PlayerCamera>>) -> Option<Listener> {
    camera.single().ok().map(Listener::of)
}

/// Starts everything in [`Sounds`]: `RenderGlobal.playSound`'s distance
/// check, then `SoundManager.playSound`.
pub(super) fn play_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut library: ResMut<SoundLibrary>,
    mut playings: ResMut<Assets<Positioned>>,
    settings: Option<Res<GameSettings>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    emitters: Query<(), With<Emitter>>,
) {
    let listener = listener(&camera);
    let mut playing = emitters.iter().len();
    for request in sounds.drain() {
        // A category that is turned off starts nothing, as Beta's Sound
        // option at zero does.
        let category = SoundCategory::of(request.name, request.position.is_some());
        let master = settings
            .as_ref()
            .map_or(1.0, |settings| settings.gain(category));
        if master <= 0.0 {
            continue;
        }
        if let Some(position) = request.position {
            let range = hearing_range(request.volume);
            if listener.is_none_or(|listener| {
                listener.position.distance_squared(position) >= range * range
            }) {
                continue;
            }
        }
        if playing >= MAX_QUEUED {
            continue;
        }
        let Some(source) = library.pick(request.name) else {
            continue;
        };
        let gains = channel_gains(listener, request.position, request.volume);
        let gains = Arc::new(ChannelGains::new(gains.map(|gain| gain * master)));
        let mut sound = commands.spawn((
            AudioPlayer(playings.add(Positioned {
                source,
                gains: Arc::clone(&gains),
            })),
            PlaybackSettings::DESPAWN.with_speed(request.pitch.clamp(MIN_PITCH, MAX_PITCH)),
            Emitter {
                position: request.position,
                volume: request.volume,
                category,
                gains,
            },
            Unstarted::default(),
        ));
        if request.position.is_some() {
            sound.insert(WorldSound);
        }
        playing += 1;
    }
}

/// Follows the listener: each playing sound's gains from where they are now.
pub(super) fn update_emitters(
    settings: Option<Res<GameSettings>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    emitters: Query<&Emitter>,
) {
    let listener = listener(&camera);
    for emitter in &emitters {
        let master = settings
            .as_ref()
            .map_or(1.0, |settings| settings.gain(emitter.category));
        let gains = channel_gains(listener, emitter.position, emitter.volume);
        emitter.gains.set(gains.map(|gain| gain * master));
    }
}

/// Removes players Bevy never started.
pub(super) fn drop_unstarted(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut waiting: Query<(Entity, &mut Unstarted), Without<AudioSink>>,
) {
    for (entity, mut unstarted) in &mut waiting {
        unstarted.0 += time.delta_secs();
        if unstarted.0 > START_TIMEOUT {
            commands.entity(entity).despawn();
        }
    }
}
