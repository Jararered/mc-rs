//! Sounds the simulation asks for: Beta's `World.playSoundEffect`,
//! `playSoundAtEntity` and `SoundManager.playSoundFX`, queued by name for the
//! client (`audio::SoundPlugin`) to play. Nothing here touches an audio
//! device, and an app without the [`Sounds`] resource simply plays nothing:
//! producers take `Option<ResMut<Sounds>>`, as they do `EffectParticles`.
//!
//! Beta draws a sound's pitch from whichever `Random` is at hand (the
//! entity's, the world's). Those draws come from [`Sounds::random`] here, so a
//! world simulates the same with and without sound.

use bevy::prelude::*;

use crate::block::definition::StepSound;
use crate::random::JavaRandom;

/// `SoundManager` names its sources `sound_0` to `sound_255` and reuses them
/// in turn, so no more than this many can be started before one is replaced.
pub const MAX_QUEUED: usize = 256;

/// What kind of sound one is, for the Audio options: each has a volume of its
/// own under the master volume. Beta has only Music and Sound; the rest are
/// the categories later versions sort the same files into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundCategory {
    /// Everything, music included. A menu's click answers to this alone.
    Master,
    Music,
    /// Blocks dug, broken, placed and walked on, switches, pistons, doors,
    /// fuses and explosions.
    Blocks,
    /// Rain and thunder.
    Weather,
    HostileMobs,
    PassiveMobs,
    /// A player's own noises: being hurt, the bow, a throw, a pickup, a
    /// splash.
    Players,
    /// Cave noises, fire, running water and portals.
    Ambient,
    /// Note blocks, and the record in a jukebox.
    NoteBlocks,
}

impl SoundCategory {
    pub const ALL: [Self; 9] = [
        Self::Master,
        Self::Music,
        Self::Blocks,
        Self::Weather,
        Self::HostileMobs,
        Self::PassiveMobs,
        Self::Players,
        Self::Ambient,
        Self::NoteBlocks,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Master => "Master volume",
            Self::Music => "Music",
            Self::Blocks => "Blocks",
            Self::Weather => "Weather",
            Self::HostileMobs => "Hostile mobs",
            Self::PassiveMobs => "Passive mobs",
            Self::Players => "Players",
            Self::Ambient => "Ambient",
            Self::NoteBlocks => "Jukebox and note blocks",
        }
    }

    /// The category of the sound Beta calls `name`. `placed` is whether it
    /// has a position: `random.click` is a lever in the world and a button
    /// in a menu.
    pub fn of(name: &str, placed: bool) -> Self {
        const HOSTILE: [&str; 6] = ["zombie", "skeleton", "spider", "creeper", "slime", "ghast"];
        const PLAYERS: [&str; 5] = ["hurt", "bow", "drr", "pop", "splash"];
        let (pool, rest) = name.split_once('.').unwrap_or((name, ""));
        match pool {
            "note" => Self::NoteBlocks,
            "mob" if HOSTILE.iter().any(|mob| rest.starts_with(mob)) => Self::HostileMobs,
            "mob" => Self::PassiveMobs,
            "ambient" if rest.starts_with("weather") => Self::Weather,
            "ambient" | "portal" | "liquid" => Self::Ambient,
            "fire" if rest == "fire" => Self::Ambient,
            "random" if PLAYERS.contains(&rest) => Self::Players,
            "random" if rest == "click" && !placed => Self::Master,
            _ => Self::Blocks,
        }
    }
}

/// A sound's pitch, where the producer has no `Random` to draw it with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pitch {
    Fixed(f32),
    /// `base + nextFloat() * span`.
    Up(f32, f32),
    /// `base + (nextFloat() - nextFloat()) * span`.
    Around(f32, f32),
}

impl Pitch {
    /// `(rand.nextFloat() - rand.nextFloat()) * 0.2F + 1.0F`, the pitch of
    /// nearly every mob's voice.
    pub const VOICE: Self = Self::Around(1.0, 0.2);

    fn resolve(self, random: &mut JavaRandom) -> f32 {
        match self {
            Self::Fixed(pitch) => pitch,
            Self::Up(base, span) => base + random.next_float() * span,
            Self::Around(base, span) => base + (random.next_float() - random.next_float()) * span,
        }
    }
}

impl From<f32> for Pitch {
    fn from(pitch: f32) -> Self {
        Self::Fixed(pitch)
    }
}

/// One sound to start, named as Beta's `SoundPool` names it (`step.stone`,
/// `mob.zombiehurt`): the file's path without its extension or trailing
/// digits, with dots for slashes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundRequest {
    pub name: &'static str,
    /// Where it sounds. `None` is `playSoundFX`: at the listener, wherever
    /// they are, and at a quarter of the volume.
    pub position: Option<Vec3>,
    /// Above 1 the sound is no louder, but carries `16 * volume` blocks.
    pub volume: f32,
    pub pitch: f32,
}

/// The sounds asked for since the client last drained them.
#[derive(Resource, Debug)]
pub struct Sounds {
    queue: Vec<SoundRequest>,
    random: JavaRandom,
}

impl Default for Sounds {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            random: JavaRandom::new(0),
        }
    }
}

impl Sounds {
    /// `World.playSoundEffect`, and `playSoundAtEntity` at an entity's feet.
    pub fn play(
        &mut self,
        name: &'static str,
        position: Vec3,
        volume: f32,
        pitch: impl Into<Pitch>,
    ) {
        self.push(name, Some(position), volume, pitch.into());
    }

    /// A block set down or broken: `ItemBlock.onItemUse` and aux effect 2001
    /// play its sound from the middle of the cell at `(volume + 1) / 2` and
    /// four fifths of its pitch.
    pub fn play_block(&mut self, name: &'static str, sound: StepSound, cell: IVec3) {
        self.play(
            name,
            cell.as_vec3() + Vec3::splat(0.5),
            (sound.volume() + 1.0) / 2.0,
            sound.pitch() * 0.8,
        );
    }

    /// `Entity.onEntityUpdate`'s splash as a body enters water, louder the
    /// faster it went in. `motion` is in blocks per tick.
    pub fn splash(&mut self, feet: Vec3, motion: Vec3) {
        let speed =
            (motion.x * motion.x * 0.2 + motion.y * motion.y + motion.z * motion.z * 0.2).sqrt();
        self.play(
            "random.splash",
            feet,
            (speed * 0.2).min(1.0),
            Pitch::Around(1.0, 0.4),
        );
    }

    /// `Entity.moveEntity`'s hiss as water or rain puts a burning body out.
    pub fn quench(&mut self, feet: Vec3) {
        self.play("random.fizz", feet, 0.7, Pitch::Around(1.6, 0.4));
    }

    /// `onCollideWithPlayer`'s pop as an item or a spent arrow is picked up.
    pub fn pop(&mut self, position: Vec3) {
        self.play("random.pop", position, 0.2, Pitch::Around(2.0, 1.4));
    }

    /// `SoundManager.playSoundFX`: a sound with no place in the world.
    pub fn play_ui(&mut self, name: &'static str, volume: f32, pitch: impl Into<Pitch>) {
        self.push(name, None, volume, pitch.into());
    }

    fn push(&mut self, name: &'static str, position: Option<Vec3>, volume: f32, pitch: Pitch) {
        // The pitch is drawn even for a sound that is dropped, as Java
        // evaluates the argument before the call.
        let pitch = pitch.resolve(&mut self.random);
        if volume <= 0.0 || self.queue.len() >= MAX_QUEUED {
            return;
        }
        self.queue.push(SoundRequest {
            name,
            position,
            volume,
            pitch,
        });
    }

    /// The `Random` a pitch is drawn from where Beta's expression is not one
    /// of [`Pitch`]'s shapes.
    pub fn random(&mut self) -> &mut JavaRandom {
        &mut self.random
    }

    pub fn queued(&self) -> &[SoundRequest] {
        &self.queue
    }

    pub fn drain(&mut self) -> std::vec::Drain<'_, SoundRequest> {
        self.queue.drain(..)
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}
