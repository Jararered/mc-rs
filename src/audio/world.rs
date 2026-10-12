//! The sounds a client works out for itself from what the world reports, as
//! Beta's `RenderGlobal` and `NetClientHandler` do: numbered aux effects,
//! note blocks, explosions, thunder, a body's cry when it is hurt, and the
//! cave noises of `World.updateBlocksAndPlayCaveSounds`.

use bevy::prelude::*;

use super::time_seed;
use crate::block::blocks::Block;
use crate::entity::EntitySize;
use crate::entity::combat::PlayerCombat;
use crate::entity::creature::Living;
use crate::entity::explosion::Exploded;
use crate::entity::mobs::Mob;
use crate::entity::voice::PLAYER_HURT;
use crate::entity::voice::voice;
use crate::player::LocalPlayer;
use crate::player::Player;
use crate::random::JavaRandom;
use crate::world::block_ticks::AuxEffect;
use crate::world::block_ticks::NotePlayed;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
use crate::world::sound::Pitch;
use crate::world::sound::Sounds;
use crate::world::tick::WorldTick;
use crate::world::weather::LightningStrike;

/// `BlockNote.playBlock`'s instruments, by the block under the note block.
const NOTE_INSTRUMENTS: [&str; 5] = [
    "note.harp",
    "note.bd",
    "note.snare",
    "note.hat",
    "note.bassattack",
];

/// `BlockFire`'s and `ItemBucket`'s hiss: `2.6F + (nextFloat() - nextFloat()) * 0.8F`.
pub const FIZZ: Pitch = Pitch::Around(2.6, 0.8);

/// `RenderGlobal.playAuxSFX`'s sounds, and `BlockNote.playBlock`.
pub fn block_sounds(
    mut sounds: ResMut<Sounds>,
    mut aux: MessageReader<AuxEffect>,
    mut notes: MessageReader<NotePlayed>,
) {
    for effect in aux.read() {
        let corner = effect.position.as_vec3();
        let center = corner + Vec3::splat(0.5);
        match effect.effect {
            1000 => sounds.play("random.click", corner, 1.0, 1.0),
            1001 => sounds.play("random.click", corner, 1.0, 1.2),
            1002 => sounds.play("random.bow", corner, 1.0, 1.2),
            1003 => {
                let name = if sounds.random().next_double() < 0.5 {
                    "random.door_open"
                } else {
                    "random.door_close"
                };
                sounds.play(name, center, 1.0, Pitch::Up(0.9, 0.1));
            }
            1004 => sounds.play("random.fizz", center, 0.5, FIZZ),
            _ => {}
        }
    }
    for note in notes.read() {
        let Some(name) = NOTE_INSTRUMENTS.get(usize::from(note.instrument)) else {
            continue;
        };
        let pitch = 2f32.powf((f32::from(note.pitch) - 12.0) / 12.0);
        sounds.play(name, note.position.as_vec3() + Vec3::splat(0.5), 3.0, pitch);
    }
}

/// `Explosion.doExplosionB` and `EntityLightningBolt`.
pub fn blast_sounds(
    mut sounds: ResMut<Sounds>,
    mut explosions: MessageReader<Exploded>,
    mut strikes: MessageReader<LightningStrike>,
) {
    for explosion in explosions.read() {
        let random = sounds.random();
        let pitch = (1.0 + (random.next_float() - random.next_float()) * 0.2) * 0.7;
        sounds.play("random.explode", explosion.center, 4.0, pitch);
    }
    for &LightningStrike(position) in strikes.read() {
        sounds.play(
            "ambient.weather.thunder",
            position,
            10000.0,
            Pitch::Up(0.8, 0.2),
        );
        sounds.play("random.explode", position, 2.0, Pitch::Up(0.5, 0.2));
    }
}

/// The `hurt_time` a body's cry last answered.
#[derive(Component, Default)]
pub struct Heard(i16);

impl Heard {
    /// Whether `hurt_time` has risen since the last look: a hit that opened
    /// a new invulnerability window, the only kind Beta gives a sound.
    fn hears(heard: Option<Mut<Heard>>, hurt_time: i16) -> (bool, bool) {
        match heard {
            Some(mut heard) => {
                let fresh = hurt_time > heard.0;
                heard.0 = hurt_time;
                (fresh, false)
            }
            None => (hurt_time > 0, true),
        }
    }
}

/// `EntityLiving.attackEntityFrom`'s cry, heard the way a multiplayer client
/// hears it (`handleHealthUpdate`): from `hurt_time` starting over.
pub fn voices(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut mobs: Query<
        (
            Entity,
            &Mob,
            &Living,
            &Transform,
            &EntitySize,
            Option<&mut Heard>,
        ),
        Without<Player>,
    >,
    mut players: Query<
        (
            Entity,
            &PlayerCombat,
            &Transform,
            &EntitySize,
            Option<&mut Heard>,
        ),
        With<Player>,
    >,
) {
    for (entity, mob, living, transform, size, heard) in &mut mobs {
        let (fresh, unseen) = Heard::hears(heard, living.hurt_time);
        if unseen {
            commands.entity(entity).try_insert(Heard(living.hurt_time));
        }
        if !fresh {
            continue;
        }
        let voice = voice(mob.kind);
        let name = if mob.health <= 0 {
            voice.death
        } else {
            voice.hurt
        };
        if let Some(name) = name {
            let feet = transform.translation - Vec3::Y * size.y_offset;
            sounds.play(name, feet, voice.volume, Pitch::VOICE);
        }
    }
    for (entity, combat, transform, size, heard) in &mut players {
        let (fresh, unseen) = Heard::hears(heard, combat.hurt_time);
        if unseen {
            commands.entity(entity).try_insert(Heard(combat.hurt_time));
        }
        if fresh {
            let feet = transform.translation - Vec3::Y * size.y_offset;
            sounds.play(PLAYER_HURT, feet, 1.0, Pitch::VOICE);
        }
    }
}

/// `World.soundCounter`: ticks until a dark cave near the player may sound.
#[derive(Resource)]
pub struct CaveAmbience {
    countdown: u32,
}

impl Default for CaveAmbience {
    fn default() -> Self {
        Self {
            countdown: JavaRandom::new(time_seed()).next_int(12000),
        }
    }
}

/// The cave half of `World.updateBlocksAndPlayCaveSounds`. Beta tries one
/// random cell in every ticked chunk; only a cell within 8 blocks of a
/// player can sound, so the chunks around the player are the ones tried.
pub fn cave_ambience(
    tick: Res<WorldTick>,
    mut sounds: ResMut<Sounds>,
    mut ambience: ResMut<CaveAmbience>,
    chunks: Option<Res<WorldChunks>>,
    light: Option<Res<LightCache>>,
    player: Query<&Transform, With<LocalPlayer>>,
) {
    let (Some(chunks), Some(light), Ok(player)) = (chunks, light, player.single()) else {
        return;
    };
    let eye = player.translation;
    let home = ChunkPosition::from_block(eye.x.floor() as i32, eye.z.floor() as i32);
    for _ in 0..tick.ticks_this_frame() {
        if ambience.countdown > 0 {
            ambience.countdown -= 1;
            continue;
        }
        'chunks: for dz in -1..=1 {
            for dx in -1..=1 {
                let random = sounds.random();
                let x =
                    (home.x + dx) * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as u32) as i32;
                let z =
                    (home.z + dz) * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as u32) as i32;
                let y = random.next_int(CHUNK_HEIGHT as u32) as i32;
                if chunks.block_at(x, y, z) != Some(Block::Air) {
                    continue;
                }
                // Dark, and out of the sky's reach.
                let dimmest = random.next_int(8) as u8;
                let Some((sky, block)) = light.channels(x, y, z) else {
                    continue;
                };
                if sky > 0 || block > dimmest {
                    continue;
                }
                let cell = IVec3::new(x, y, z).as_vec3() + Vec3::splat(0.5);
                let distance = eye.distance_squared(cell);
                if distance > 4.0 && distance < 64.0 {
                    sounds.play("ambient.cave.cave", cell, 0.7, Pitch::Up(0.8, 0.2));
                    ambience.countdown = sounds.random().next_int(12000) + 6000;
                    break 'chunks;
                }
            }
        }
    }
}
