//! Overworld's 20 Hz rain, snow and lightning state, shared by simulation and rendering.
use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use crate::app::state::AppScreen;
use crate::block::id::Id;
use crate::block::properties::is_opaque_cube;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobKind;
use crate::entity::mobs::SpawnMob;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::random::JavaRandom;
use crate::world::biome::Biome;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::WorldChunks;
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
    pub fn skylight_penalty(&self) -> u8 {
        (self.rain_strength * 3.0 + self.thunder_strength * 5.0).round() as u8
    }
    pub fn is_raining(&self) -> bool {
        self.rain_strength > 0.2
    }
    pub fn is_thundering(&self) -> bool {
        self.thunder_strength * self.rain_strength > 0.9
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
) {
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
            Biome::Taiga | Biome::Tundra | Biome::IceDesert | Biome::Desert
        )
    }) {
        return false;
    }
    (y..CHUNK_HEIGHT as i32).all(|above| !chunks.block_at(x, above, z).is_some_and(is_opaque_cube))
}

fn apply_lightning(
    mut commands: Commands,
    mut strikes: MessageReader<LightningStrike>,
    mut chunks: ResMut<WorldChunks>,
    mut ticks: ResMut<crate::world::block_ticks::BlockTicks>,
    mut streaming: Option<ResMut<crate::world::streaming::WorldStreaming>>,
    mut persistence: Option<ResMut<crate::world::persistence::WorldPersistence>>,
    mut mobs: Query<(Entity, &mut Mob, &Transform)>,
    mut player: Query<(&Transform, &mut PlayerHealth), With<Player>>,
    mut spawn: MessageWriter<SpawnMob>,
) {
    for &LightningStrike(center) in strikes.read() {
        let at = center.floor().as_ivec3();
        if chunks.block_at(at.x, at.y, at.z) == Some(Id::Air)
            && chunks
                .block_at(at.x, at.y - 1, at.z)
                .is_some_and(is_opaque_cube)
        {
            let old_meta = chunks.metadata_at(at.x, at.y, at.z);
            if let Some(old) = chunks.set_block(at.x, at.y, at.z, Id::Fire) {
                ticks.block_changed(at, old, old_meta);
                if let Some(s) = streaming.as_deref_mut() {
                    s.request_block_update(at.x, at.y, at.z);
                }
                if let Some(p) = persistence.as_deref_mut() {
                    p.mark_dirty(crate::world::chunk::ChunkPosition::from_block(at.x, at.z));
                }
            }
        }
        for (entity, mut mob, transform) in &mut mobs {
            if transform.translation.distance_squared(center) > 9.0 {
                continue;
            }
            if mob.kind == MobKind::Pig {
                commands.entity(entity).despawn();
                spawn.write(SpawnMob {
                    kind: MobKind::PigZombie,
                    feet: transform.translation,
                    explicit: true,
                    variant: 0,
                });
            } else if mob.kind == MobKind::Creeper {
                mob.charged = true;
            } else {
                mob.fire_ticks = 160;
                mob.health -= 5;
            }
        }
        if let Ok((transform, mut health)) = player.single_mut()
            && transform.translation.distance_squared(center) <= 9.0
        {
            health.current = health.current.saturating_sub(5);
        }
    }
}
