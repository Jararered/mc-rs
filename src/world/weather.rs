//! Overworld's 20 Hz rain, snow and lightning state, shared by simulation and rendering.
use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use crate::app::settings::Difficulty;
use crate::app::state::AppScreen;
use crate::block::blocks::Block;
use crate::entity::Velocity;
use crate::entity::combat::Hit;
use crate::entity::combat::PlayerCombat;
use crate::entity::combat::drop_loot;
use crate::entity::combat::hurt_creature;
use crate::entity::combat::hurt_player;
use crate::entity::creature::Living;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobType;
use crate::entity::mobs::SpawnMob;
use crate::entity::projectiles::victim;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::random::ItemRng;
use crate::random::JavaRandom;
use crate::world::biome::Biome;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::WorldChunks;
use crate::world::environment::skylight_subtracted_in_weather;
use crate::world::lighting::LightCache;
use crate::world::tick::WorldTick;

#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldWeather {
    pub raining: bool,
    pub thundering: bool,
    pub rain_time: u32,
    pub thunder_time: u32,
    pub rain_strength: f32,
    pub thunder_strength: f32,
    pub rng_state: u64,
}

impl Default for WorldWeather {
    fn default() -> Self {
        Self {
            raining: false,
            thundering: false,
            rain_time: 0,
            thunder_time: 0,
            rain_strength: 0.0,
            thunder_strength: 0.0,
            rng_state: JavaRandom::new(0x5745_4154).state(),
        }
    }
}

impl WorldWeather {
    /// `World.getWeightedThunderStrength`.
    pub fn weighted_thunder(&self) -> f32 {
        self.thunder_strength * self.rain_strength
    }
    /// `World.calculateSkylightSubtracted` under this weather.
    pub fn skylight_subtracted(&self, angle: f32) -> u8 {
        skylight_subtracted_in_weather(angle, self.rain_strength, self.weighted_thunder())
    }
    pub fn is_raining(&self) -> bool {
        self.rain_strength > 0.2
    }
    pub fn is_thundering(&self) -> bool {
        self.weighted_thunder() > 0.9
    }
    /// `World.stopPrecipitation`, for the morning after a night slept
    /// through. The strengths fade out on their own.
    pub fn stop_precipitation(&mut self) {
        self.rain_time = 0;
        self.raining = false;
        self.thunder_time = 0;
        self.thundering = false;
    }
    pub fn step(&mut self) {
        let mut rng = JavaRandom::from_state(self.rng_state);
        if self.rain_time == 0 {
            self.rain_time = if self.raining {
                12_000 + rng.next_int(12_000)
            } else {
                12_000 + rng.next_int(168_000)
            };
        } else {
            self.rain_time -= 1;
            if self.rain_time == 0 {
                self.raining = !self.raining;
            }
        }
        if self.thunder_time == 0 {
            self.thunder_time = if self.thundering {
                3600 + rng.next_int(12_000)
            } else {
                12_000 + rng.next_int(168_000)
            };
        } else {
            self.thunder_time -= 1;
            if self.thunder_time == 0 {
                self.thundering = !self.thundering;
            }
        }
        self.rain_strength =
            (self.rain_strength + if self.raining { 0.01 } else { -0.01 }).clamp(0., 1.);
        self.thunder_strength =
            (self.thunder_strength + if self.thundering { 0.01 } else { -0.01 }).clamp(0., 1.);
        self.rng_state = rng.state();
    }
}

/// Skylight subtracted at `angle`, with clear skies when there is no weather.
pub fn skylight_subtracted(weather: Option<&WorldWeather>, angle: f32) -> u8 {
    weather.map_or_else(
        || crate::world::environment::skylight_subtracted(angle),
        |weather| weather.skylight_subtracted(angle),
    )
}

#[derive(Message, Clone, Copy, Debug)]
pub struct LightningStrike(pub Vec3);

pub struct WeatherPlugin;
impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldWeather>()
            .add_message::<LightningStrike>()
            .add_systems(
                Update,
                (tick_weather, apply_lightning)
                    .chain()
                    .before(crate::world::block_ticks::BlockTickSet)
                    .run_if(|state: Option<Res<State<AppScreen>>>| {
                        state.is_none_or(|s| *s.get() == AppScreen::Playing)
                    }),
            );
    }
}

fn tick_weather(
    tick: Res<WorldTick>,
    mut weather: ResMut<WorldWeather>,
    player: Query<&Transform, With<Player>>,
    chunks: Res<WorldChunks>,
    light: Res<LightCache>,
    mut strikes: MessageWriter<LightningStrike>,
    dimension: Option<Res<crate::world::dimension::ActiveDimension>>,
) {
    // `World.updateWeather` does nothing where `hasNoSky`: the shared rain
    // and thunder counters hold still while the player is in the Nether.
    if dimension.is_some_and(|dimension| !dimension.0.has_weather()) {
        return;
    }
    let Ok(player) = player.single() else {
        return;
    };
    let center =
        crate::world::chunk::ChunkPosition::from_world(player.translation.x, player.translation.z);
    for _ in 0..tick.ticks_this_frame() {
        weather.step();
        if !weather.is_thundering() {
            continue;
        }
        let mut rng = JavaRandom::from_state(weather.rng_state);
        for pos in chunks.positions() {
            if (pos.x - center.x).abs() > 8
                || (pos.z - center.z).abs() > 8
                || !light.contains(pos)
                || rng.next_int(100_000) != 0
            {
                continue;
            }
            let x = pos.x * 16 + rng.next_int(16) as i32;
            let z = pos.z * 16 + rng.next_int(16) as i32;
            let y = chunks.get(pos).map_or(0, |c| {
                c.heightmap
                    .get(x.rem_euclid(16) as usize, z.rem_euclid(16) as usize)
                    as i32
            });
            if can_strike(&chunks, x, y, z) {
                strikes.write(LightningStrike(Vec3::new(
                    x as f32 + 0.5,
                    y as f32,
                    z as f32 + 0.5,
                )));
            }
        }
        weather.rng_state = rng.state();
    }
}

pub fn can_strike(chunks: &WorldChunks, x: i32, y: i32, z: i32) -> bool {
    if !(1..CHUNK_HEIGHT as i32).contains(&y) {
        return false;
    }
    if chunks.climate_at(x, z).is_none_or(|c| {
        matches!(
            c.biome,
            Biome::Taiga | Biome::Tundra | Biome::IceDesert | Biome::Desert | Biome::Hell
        )
    }) {
        return false;
    }
    (y..CHUNK_HEIGHT as i32).all(|above| {
        !chunks
            .block_at(x, above, z)
            .is_some_and(Block::is_opaque_cube)
    })
}

fn apply_lightning(
    mut commands: Commands,
    mut strikes: MessageReader<LightningStrike>,
    mut chunks: ResMut<WorldChunks>,
    mut ticks: ResMut<crate::world::block_ticks::BlockTicks>,
    mut streaming: Option<ResMut<crate::world::streaming::WorldStreaming>>,
    mut persistence: Option<ResMut<crate::world::persistence::WorldPersistence>>,
    mut mobs: Query<(Entity, &mut Mob, &mut Living, &mut Velocity, &Transform), Without<Player>>,
    mut player: Query<
        (
            &Transform,
            Option<&mut PlayerHealth>,
            Option<&mut PlayerCombat>,
            &mut Velocity,
            Option<&mut Inventory>,
        ),
        With<Player>,
    >,
    mut survival: Query<&mut crate::player::PlayerSurvival, With<Player>>,
    mut spawn: MessageWriter<SpawnMob>,
    mut loot: Local<ItemRng>,
    mut spare_armor: Local<[Option<ItemStack>; 4]>,
) {
    let mut player = player.single_mut().ok();
    for &LightningStrike(center) in strikes.read() {
        let at = center.floor().as_ivec3();
        if chunks.block_at(at.x, at.y, at.z) == Some(Block::Air)
            && chunks
                .block_at(at.x, at.y - 1, at.z)
                .is_some_and(Block::is_opaque_cube)
        {
            let old_meta = chunks.metadata_at(at.x, at.y, at.z);
            if let Some(old) = chunks.set_block(at.x, at.y, at.z, Block::Fire) {
                ticks.block_changed(at, old, old_meta);
                if let Some(s) = streaming.as_deref_mut() {
                    s.request_block_update(at.x, at.y, at.z);
                }
                if let Some(p) = persistence.as_deref_mut() {
                    p.mark_dirty(crate::world::chunk::ChunkPosition::from_block(at.x, at.z));
                }
            }
        }
        for (entity, mut mob, mut living, mut velocity, transform) in &mut mobs {
            let feet = transform.translation;
            if feet.distance_squared(center) > 9.0 {
                continue;
            }
            if mob.kind == MobType::Pig {
                commands.entity(entity).despawn();
                spawn.write(SpawnMob {
                    kind: MobType::PigZombie,
                    feet,
                    variant: 0,
                });
                continue;
            }
            if mob.kind == MobType::Creeper {
                mob.charged = true;
            }
            // `Entity.onStruckByLightning`: five points of fire damage, and
            // the body catches fire.
            if !matches!(mob.kind, MobType::Ghast | MobType::PigZombie) {
                let wound = hurt_creature(
                    &mut mob,
                    &mut living,
                    &mut velocity,
                    feet,
                    Hit::environment(5),
                );
                if wound.died {
                    drop_loot(&mut commands, &mut loot, &mut mob, feet);
                }
            }
            // Beta's `++fire; if (fire == 0) fire = 300`, which counts on an
            // idle mob resting at -1. One that has not ticked yet is still at 0.
            if mob.fire_ticks <= 0 {
                mob.fire_ticks = 300;
            } else {
                mob.fire_ticks += 1;
            }
        }
        if player
            .as_ref()
            .is_some_and(|(transform, ..)| transform.translation.distance_squared(center) <= 9.0)
            && let Some(mut victim) = victim(&mut player, &mut spare_armor)
        {
            hurt_player(
                &mut victim,
                Hit::environment(5),
                Difficulty::Normal,
                &mut loot,
            );
            // `++fire; if (fire == 0) fire = 300`. A player rests at -20, so
            // it is the fire the bolt leaves behind that sets them alight.
            if let Ok(mut survival) = survival.single_mut() {
                survival.fire += 1;
                if survival.fire == 0 {
                    survival.fire = 300;
                }
            }
        }
    }
}
