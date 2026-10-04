//! Mob simulation cost per world tick: `cargo bench --bench entities`.
//!
//! Each scenario runs the world `Update` schedule headless against hand-built
//! hills, the way `tests/world/mobs.rs` drives mobs. It reports the whole
//! tick, the share `tick_creatures` took, and the pathfinder's searches. Every
//! scenario runs at midnight, so monsters hunt and nothing burns.

use std::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use game::app::settings::GameSettings;
use game::block::blocks::Block;
use game::entity::EntityDiagnostics;
use game::entity::EntitySize;
use game::entity::mobs::Mob;
use game::entity::mobs::MobType;
use game::entity::mobs::spawn;
use game::entity::pathfinding::Pathfinder;
use game::player::Player;
use game::random::JavaRandom;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;
use game::world::lighting::LightCache;
use game::world::plugin::WorldPlugin;
use game::world::tick::WorldTick;

/// Chunks loaded on each side of the origin.
const RADIUS: i32 = 4;
/// Natural spawning reaches 8 chunks out, and each of those needs its
/// neighbors loaded.
const SPAWNING_RADIUS: i32 = 9;
const WARMUP_TICKS: u32 = 100;
const MEASURED_TICKS: u32 = 400;
/// The pillar the player stands on in the chase scenarios.
const PILLAR: i32 = 6;
/// `/time set midnight`.
const MIDNIGHT: u64 = 18_000;

/// Gentle hills of grass over dirt and stone.
fn surface(x: i32, z: i32) -> i32 {
    64 + ((x as f32 * 0.21).sin() * 2.5 + (z as f32 * 0.17).cos() * 2.5) as i32
}

fn hills(radius: i32) -> WorldChunks {
    let mut chunks = WorldChunks::default();
    for cx in -radius..=radius {
        for cz in -radius..=radius {
            let mut chunk = Chunk::new();
            for x in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    let top = surface(cx * 16 + x as i32, cz * 16 + z as i32) as usize;
                    for y in 0..top {
                        let block = if y + 3 < top {
                            Block::Stone
                        } else {
                            Block::Dirt
                        };
                        chunk.set(x, y, z, block);
                    }
                    chunk.set(x, top, z, Block::Grass);
                }
            }
            chunks.insert(
                ChunkPosition { x: cx, z: cz },
                GeneratedChunk {
                    heightmap: Heightmap::from_chunk(&chunk),
                    biomes: BiomeMap::from_cells(
                        [Climate {
                            temperature: 0.5,
                            humidity: 0.5,
                            biome: Biome::Forest,
                        }; CHUNK_SIZE * CHUNK_SIZE],
                    ),
                    chunk,
                    items: Vec::new(),
                    populated: true,
                },
            );
        }
    }
    chunks
}

/// A stone pillar at the origin, `PILLAR` blocks above the ground. Returns
/// the feet of a player standing on it.
fn pillar(chunks: &mut WorldChunks) -> Vec3 {
    let ground = surface(0, 0);
    for y in ground + 1..=ground + PILLAR {
        chunks.set_block(0, y, 0, Block::Stone);
    }
    Vec3::new(0.5, (ground + PILLAR + 1) as f32, 0.5)
}

/// A headless world at midnight whose player has no health, so mobs can
/// chase it for the whole run without killing it. A zero render distance
/// leaves no chunks for natural spawning, so the mob count stays put.
fn app(chunks: WorldChunks, player_feet: Vec3) -> App {
    app_with_spawning(chunks, player_feet, 0)
}

fn app_with_spawning(chunks: WorldChunks, player_feet: Vec3, render_distance: i32) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(WorldPlugin);
    app.insert_resource(GameSettings {
        render_distance,
        ..default()
    });
    app.world_mut()
        .resource_mut::<WorldTick>()
        .set_world_time(MIDNIGHT);
    *app.world_mut().resource_mut::<WorldChunks>() = chunks;
    app.world_mut().spawn((
        Player,
        Transform::from_translation(player_feet + Vec3::Y * EntitySize::PLAYER.y_offset),
    ));
    app
}

/// Place a mob on the highest ground its box could overlap.
fn summon(app: &mut App, mob: Mob, x: f32, z: f32) {
    let reach = [-1.0, 1.0];
    let ground = reach
        .iter()
        .flat_map(|dx| reach.iter().map(move |dz| (x + dx, z + dz)))
        .map(|(x, z)| surface(x.floor() as i32, z.floor() as i32))
        .max()
        .unwrap_or(64);
    let feet = Vec3::new(x, (ground + 1) as f32, z);
    spawn(&mut app.world_mut().commands(), mob, feet);
    app.world_mut().flush();
}

/// `count` mobs of the given kinds, round-robin, scattered in a ring around
/// the origin.
fn scatter(app: &mut App, kinds: &[MobType], count: usize, ring: (f32, f32), seed: u64) {
    let mut rng = JavaRandom::new(seed);
    for i in 0..count {
        let kind = kinds[i % kinds.len()];
        let angle = rng.next_float() * std::f32::consts::TAU;
        let distance = ring.0 + rng.next_float() * (ring.1 - ring.0);
        let mut mob = Mob::new(kind, seed + i as u64);
        if kind == MobType::Wolf {
            mob.tamed = true;
            mob.owner = Some("Player".into());
        }
        summon(
            app,
            mob,
            angle.cos() * distance + 0.5,
            angle.sin() * distance + 0.5,
        );
    }
}

fn run(name: &str, mut app: App) {
    let step = |app: &mut App| {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.world_mut().run_schedule(Update);
    };
    for _ in 0..WARMUP_TICKS {
        step(&mut app);
    }
    app.world_mut().resource_mut::<EntityDiagnostics>().take();
    let mut samples = Vec::with_capacity(MEASURED_TICKS as usize);
    for _ in 0..MEASURED_TICKS {
        let start = Instant::now();
        step(&mut app);
        samples.push(start.elapsed());
    }
    let diagnostics = app.world_mut().resource_mut::<EntityDiagnostics>().take();
    let mobs = app
        .world_mut()
        .query_filtered::<(), With<Mob>>()
        .iter(app.world())
        .count();
    samples.sort();
    let total: Duration = samples.iter().sum();
    let mean = total / MEASURED_TICKS;
    let p99 = samples[samples.len() * 99 / 100];
    let searches = diagnostics.searches.searches;
    let nodes = diagnostics.searches.nodes;
    println!(
        "{name:<34} {mobs:>4} mobs  {:>7.1} µs/tick mean  {:>7.1} µs p99  \
         {:>7.1} µs creatures  {:>6.1} µs spawning  {:>5.2} searches/tick  \
         {:>5} nodes/search  {:>5.2} reused/tick",
        mean.as_secs_f64() * 1e6,
        p99.as_secs_f64() * 1e6,
        diagnostics.creatures.average_ms().unwrap_or(0.0) * 1e3,
        diagnostics.spawning.average_ms().unwrap_or(0.0) * 1e3,
        searches as f64 / f64::from(MEASURED_TICKS),
        nodes.checked_div(searches).unwrap_or(0),
        diagnostics.searches.reused as f64 / f64::from(MEASURED_TICKS),
    );
}

/// About a full spawn cap around one player: 79 monsters, and 21 animals in
/// place of the 16 animals and 5 squid. The player is too far away to chase.
fn wandering() -> App {
    let mut app = app(hills(RADIUS), Vec3::new(0.5, 80.0, 70.5));
    use MobType as M;
    let monsters = [M::Spider, M::Zombie, M::Skeleton, M::Creeper];
    let animals = [M::Sheep, M::Pig, M::Chicken, M::Cow];
    scatter(&mut app, &monsters, 79, (2.0, 50.0), 1);
    scatter(&mut app, &animals, 21, (2.0, 50.0), 2);
    app
}

/// Zombies and spiders around a player on a pillar they cannot climb.
fn besieged() -> App {
    let mut chunks = hills(RADIUS);
    let feet = pillar(&mut chunks);
    let mut app = app(chunks, feet);
    scatter(&mut app, &[MobType::Zombie], 20, (3.0, 12.0), 3);
    scatter(&mut app, &[MobType::Spider], 10, (3.0, 12.0), 4);
    app
}

/// Tamed wolves whose owner stands on a pillar they cannot climb.
fn stranded_wolves() -> App {
    let mut chunks = hills(RADIUS);
    let feet = pillar(&mut chunks);
    let mut app = app(chunks, feet);
    scatter(&mut app, &[MobType::Wolf], 5, (6.0, 11.0), 5);
    app
}

/// Natural spawning around a player in lit, loaded chunks, from an empty
/// world until the monster cap fills, then holding there. `loaded` chunks
/// out from the player stay loaded, as a long render distance keeps them.
fn spawning(loaded: i32) -> App {
    let chunks = hills(loaded);
    let mut light = LightCache::default();
    for position in chunks.positions().collect::<Vec<_>>() {
        if position.x.abs() <= SPAWNING_RADIUS && position.z.abs() <= SPAWNING_RADIUS {
            light.relight(&chunks, position);
        }
    }
    let mut app = app_with_spawning(chunks, Vec3::new(0.5, (surface(0, 0) + 1) as f32, 0.5), 8);
    app.insert_resource(light);
    app
}

/// One radius-16 search toward the pillar top, which explores every node.
fn exhaustive_search() {
    let mut chunks = hills(RADIUS);
    let feet = pillar(&mut chunks);
    let size = MobType::Zombie.size(0);
    let start = Vec3::new(8.5, (surface(8, 0) + 1) as f32, 0.5);
    let mut pathfinder = Pathfinder::default();
    const RUNS: u32 = 200;
    let begin = Instant::now();
    for _ in 0..RUNS {
        std::hint::black_box(pathfinder.path_to_feet(&chunks, start, size, feet, 16.0));
    }
    let elapsed = begin.elapsed() / RUNS;
    let stats = pathfinder.take_stats();
    println!(
        "{:<34}       {:>8.1} µs/search          {:>6} nodes/search",
        "exhaustive radius-16 search",
        elapsed.as_secs_f64() * 1e6,
        stats.nodes / stats.searches,
    );
}

fn main() {
    run("A: 100 mobs wandering", wandering());
    run("B: 30 monsters around a pillar", besieged());
    run("C: 5 tamed wolves, owner on pillar", stranded_wolves());
    run("D: natural spawning, 8 chunks", spawning(SPAWNING_RADIUS));
    run("E: natural spawning, 32 loaded", spawning(33));
    exhaustive_search();
}
