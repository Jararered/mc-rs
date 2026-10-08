//! Beta `ChunkProviderGenerate.populate`.
//!
//! One pass per chunk, on one random sequence, in the reference's order:
//! lakes, lava lakes, dungeons, clay, ores, trees, flowers, tall grass, dead
//! bushes, roses, mushrooms, reeds, pumpkins, cactus, springs, and snow. Every feature
//! reads and writes the live [`PopulationWorld`], so later features see
//! earlier ones exactly as Beta's do.

use crate::block::blocks::Block;
use crate::block::blocks::species;
use crate::random::JavaRandom;
use crate::world::chunk::ChunkPosition;

use super::biome::BiomeGenerator;
use super::cactus::cactus_patch;
use super::math;
use super::math::PI;
use super::plants::dead_bush_patch;
use super::plants::flower_patch;
use super::plants::tall_grass_patch;
use super::pumpkin::pumpkin_patch;
use super::reeds::reed_patch;
use super::terrain::TerrainGenerator;
use super::trees::generate_tree;
use super::trees::select_tree;
use super::world::PopulationWorld;
use super::world::is_liquid;
use super::world::is_solid;
use super::world::is_water;
use crate::world::biome::Biome;

/// `populate`'s chunk random: the world seed salted by the chunk position.
pub(in crate::world::generation) fn source_random(seed: u64, source: ChunkPosition) -> JavaRandom {
    let mut random = JavaRandom::new(seed);
    let salt_x = random.next_long() / 2 * 2 + 1;
    let salt_z = random.next_long() / 2 * 2 + 1;
    let chunk_seed = i64::from(source.x)
        .wrapping_mul(salt_x)
        .wrapping_add(i64::from(source.z).wrapping_mul(salt_z))
        ^ seed as i64;
    JavaRandom::new(chunk_seed as u64)
}

/// Populate `world.origin()`.
pub(super) fn populate(
    world: &mut PopulationWorld,
    terrain: &TerrainGenerator,
    biomes: &BiomeGenerator,
    rand: &mut JavaRandom,
) {
    let source = world.origin();
    let ox = source.x * 16;
    let oz = source.z * 16;
    let biome = biomes
        .climate_at(f64::from(ox + 16), f64::from(oz + 16))
        .biome;

    if rand.next_int(4) == 0 {
        let x = ox + rand.next_int(16) as i32 + 8;
        let y = rand.next_int(128) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        lake(world, rand, x, y, z, Block::Water);
    }
    if rand.next_int(8) == 0 {
        let x = ox + rand.next_int(16) as i32 + 8;
        let bound = rand.next_int(120) + 8;
        let y = rand.next_int(bound) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        if y < 64 || rand.next_int(10) == 0 {
            lake(world, rand, x, y, z, Block::Lava);
        }
    }
    for _ in 0..8 {
        let x = ox + rand.next_int(16) as i32 + 8;
        let y = rand.next_int(128) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        dungeon(world, rand, x, y, z);
    }
    for _ in 0..10 {
        let x = ox + rand.next_int(16) as i32;
        let y = rand.next_int(128) as i32;
        let z = oz + rand.next_int(16) as i32;
        clay(world, rand, x, y, z, 32);
    }
    const ORES: [(u32, u32, u32, Block); 7] = [
        (20, 128, 32, Block::Dirt),
        (10, 128, 32, Block::Gravel),
        (20, 128, 16, Block::CoalOre),
        (20, 64, 8, Block::IronOre),
        (2, 32, 8, Block::GoldOre),
        (8, 16, 7, Block::RedstoneOre),
        (1, 16, 7, Block::DiamondOre),
    ];
    for (count, max_y, size, block) in ORES {
        for _ in 0..count {
            let x = ox + rand.next_int(16) as i32;
            let y = rand.next_int(max_y) as i32;
            let z = oz + rand.next_int(16) as i32;
            vein(world, rand, x, y, z, size, block);
        }
    }
    {
        let x = ox + rand.next_int(16) as i32;
        let y = rand.next_int(16) as i32 + rand.next_int(16) as i32;
        let z = oz + rand.next_int(16) as i32;
        vein(world, rand, x, y, z, 6, Block::LapisOre);
    }

    let density = terrain
        .mob_spawner
        .point(f64::from(ox) * 0.5, f64::from(oz) * 0.5);
    let base_count = ((density / 8.0 + rand.next_double() * 4.0 + 4.0) / 3.0) as i32;
    let mut trees = 0;
    if rand.next_int(10) == 0 {
        trees += 1;
    }
    match biome {
        Biome::Forest | Biome::Rainforest | Biome::Taiga => trees += base_count + 5,
        Biome::SeasonalForest => trees += base_count + 2,
        Biome::Desert | Biome::Tundra | Biome::Plains => trees -= 20,
        _ => {}
    }
    for _ in 0..trees {
        let x = ox + rand.next_int(16) as i32 + 8;
        let z = oz + rand.next_int(16) as i32 + 8;
        let kind = select_tree(biome, rand);
        let y = world.height(x, z);
        generate_tree(kind, world, rand, x, y, z);
    }

    let dandelions = match biome {
        Biome::Forest | Biome::Taiga => 2,
        Biome::SeasonalForest => 4,
        Biome::Plains => 3,
        _ => 0,
    };
    for _ in 0..dandelions {
        let x = ox + rand.next_int(16) as i32 + 8;
        let y = rand.next_int(128) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        flower_patch(world, rand, x, y, z, Block::Dandelion);
    }

    let grass = match biome {
        Biome::Forest | Biome::SeasonalForest => 2,
        Biome::Rainforest | Biome::Plains => 10,
        Biome::Taiga => 1,
        _ => 0,
    };
    for _ in 0..grass {
        // The rainforest fern roll comes before the patch position.
        let metadata = if biome == Biome::Rainforest && rand.next_int(3) != 0 {
            species::FERN
        } else {
            0
        };
        let x = ox + rand.next_int(16) as i32 + 8;
        let y = rand.next_int(128) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        tall_grass_patch(world, rand, x, y, z, metadata);
    }

    if biome == Biome::Desert {
        for _ in 0..2 {
            let x = ox + rand.next_int(16) as i32 + 8;
            let y = rand.next_int(128) as i32;
            let z = oz + rand.next_int(16) as i32 + 8;
            dead_bush_patch(world, rand, x, y, z);
        }
    }

    for (chance, block) in [
        (2, Block::Rose),
        (4, Block::BrownMushroom),
        (8, Block::RedMushroom),
    ] {
        if rand.next_int(chance) == 0 {
            let x = ox + rand.next_int(16) as i32 + 8;
            let y = rand.next_int(128) as i32;
            let z = oz + rand.next_int(16) as i32 + 8;
            flower_patch(world, rand, x, y, z, block);
        }
    }

    for _ in 0..10 {
        let x = ox + rand.next_int(16) as i32 + 8;
        let y = rand.next_int(128) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        reed_patch(world, rand, x, y, z);
    }

    if rand.next_int(32) == 0 {
        let x = ox + rand.next_int(16) as i32 + 8;
        let y = rand.next_int(128) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        pumpkin_patch(world, rand, x, y, z);
    }

    if biome == Biome::Desert {
        for _ in 0..10 {
            let x = ox + rand.next_int(16) as i32 + 8;
            let y = rand.next_int(128) as i32;
            let z = oz + rand.next_int(16) as i32 + 8;
            cactus_patch(world, rand, x, y, z);
        }
    }
}

/// `WorldGenMinable`: an ellipsoid chain replacing stone.
fn vein(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
    size: u32,
    block: Block,
) {
    let [x0, x1, z0, z1, y0, y1] = vein_endpoints(rand, x, y, z, size);
    let count = f64::from(size);
    for step in 0..=size {
        let t = f64::from(step);
        let cx = x0 + (x1 - x0) * t / count;
        let cy = y0 + (y1 - y0) * t / count;
        let cz = z0 + (z1 - z0) * t / count;
        let jitter = rand.next_double() * count / 16.0;
        let bulge = f64::from(math::sin(step as f32 * PI / size as f32) + 1.0);
        let horizontal = bulge * jitter + 1.0;
        let vertical = bulge * jitter + 1.0;
        let (hx, hy) = (horizontal / 2.0, vertical / 2.0);
        for bx in math::floor_double(cx - hx)..=math::floor_double(cx + hx) {
            let nx = (f64::from(bx) + 0.5 - cx) / hx;
            if nx * nx >= 1.0 {
                continue;
            }
            for by in math::floor_double(cy - hy)..=math::floor_double(cy + hy) {
                let ny = (f64::from(by) + 0.5 - cy) / hy;
                if nx * nx + ny * ny >= 1.0 {
                    continue;
                }
                for bz in math::floor_double(cz - hx)..=math::floor_double(cz + hx) {
                    let nz = (f64::from(bz) + 0.5 - cz) / hx;
                    if nx * nx + ny * ny + nz * nz < 1.0 && world.get(bx, by, bz) == Block::Stone {
                        world.set(bx, by, bz, block);
                    }
                }
            }
        }
    }
}

/// The vein's endpoints, summed in float as Beta sums them.
fn vein_endpoints(rand: &mut JavaRandom, x: i32, y: i32, z: i32, size: u32) -> [f64; 6] {
    let angle = rand.next_float() * PI;
    let reach_x = math::sin(angle) * size as f32 / 8.0;
    let reach_z = math::cos(angle) * size as f32 / 8.0;
    let x0 = f64::from((x + 8) as f32 + reach_x);
    let x1 = f64::from((x + 8) as f32 - reach_x);
    let z0 = f64::from((z + 8) as f32 + reach_z);
    let z1 = f64::from((z + 8) as f32 - reach_z);
    let y0 = f64::from(y + rand.next_int(3) as i32 + 2);
    let y1 = f64::from(y + rand.next_int(3) as i32 + 2);
    [x0, x1, z0, z1, y0, y1]
}

/// `WorldGenClay`: the vein shape, started in water, turning sand into clay.
fn clay(world: &mut PopulationWorld, rand: &mut JavaRandom, x: i32, y: i32, z: i32, size: u32) {
    if !is_water(world.get(x, y, z)) {
        return;
    }
    let [x0, x1, z0, z1, y0, y1] = vein_endpoints(rand, x, y, z, size);
    let count = f64::from(size);
    for step in 0..=size {
        let t = f64::from(step);
        let cx = x0 + (x1 - x0) * t / count;
        let cy = y0 + (y1 - y0) * t / count;
        let cz = z0 + (z1 - z0) * t / count;
        let jitter = rand.next_double() * count / 16.0;
        let bulge = f64::from(math::sin(step as f32 * PI / size as f32) + 1.0);
        let horizontal = bulge * jitter + 1.0;
        let vertical = bulge * jitter + 1.0;
        let (hx, hy) = (horizontal / 2.0, vertical / 2.0);
        for bx in math::floor_double(cx - hx)..=math::floor_double(cx + hx) {
            for by in math::floor_double(cy - hy)..=math::floor_double(cy + hy) {
                for bz in math::floor_double(cz - hx)..=math::floor_double(cz + hx) {
                    let nx = (f64::from(bx) + 0.5 - cx) / hx;
                    let ny = (f64::from(by) + 0.5 - cy) / hy;
                    let nz = (f64::from(bz) + 0.5 - cz) / hx;
                    if nx * nx + ny * ny + nz * nz < 1.0 && world.get(bx, by, bz) == Block::Sand {
                        world.set(bx, by, bz, Block::Clay);
                    }
                }
            }
        }
    }
}

/// `WorldGenLakes`: overlapping ellipsoids in a 16×8×16 box, liquid in the
/// lower half and air above.
fn lake(world: &mut PopulationWorld, rand: &mut JavaRandom, x: i32, y: i32, z: i32, liquid: Block) {
    let x = x - 8;
    let z = z - 8;
    let mut y = y;
    while y > 0 && world.is_air(x, y, z) {
        y -= 1;
    }
    y -= 4;
    let mut shape = [false; 2048];
    let ellipsoids = rand.next_int(4) + 4;
    for _ in 0..ellipsoids {
        let rx = rand.next_double() * 6.0 + 3.0;
        let ry = rand.next_double() * 4.0 + 2.0;
        let rz = rand.next_double() * 6.0 + 3.0;
        let cx = rand.next_double() * (16.0 - rx - 2.0) + 1.0 + rx / 2.0;
        let cy = rand.next_double() * (8.0 - ry - 4.0) + 2.0 + ry / 2.0;
        let cz = rand.next_double() * (16.0 - rz - 2.0) + 1.0 + rz / 2.0;
        for bx in 1..15 {
            for bz in 1..15 {
                for by in 1..7 {
                    let dx = (f64::from(bx) - cx) / (rx / 2.0);
                    let dy = (f64::from(by) - cy) / (ry / 2.0);
                    let dz = (f64::from(bz) - cz) / (rz / 2.0);
                    if dx * dx + dy * dy + dz * dz < 1.0 {
                        shape[((bx * 16 + bz) * 8 + by) as usize] = true;
                    }
                }
            }
        }
    }
    let inside = |bx: i32, by: i32, bz: i32| shape[((bx * 16 + bz) * 8 + by) as usize];
    let boundary = |bx: i32, by: i32, bz: i32| {
        !inside(bx, by, bz)
            && (bx < 15 && inside(bx + 1, by, bz)
                || bx > 0 && inside(bx - 1, by, bz)
                || bz < 15 && inside(bx, by, bz + 1)
                || bz > 0 && inside(bx, by, bz - 1)
                || by < 7 && inside(bx, by + 1, bz)
                || by > 0 && inside(bx, by - 1, bz))
    };
    for bx in 0..16 {
        for bz in 0..16 {
            for by in 0..8 {
                if boundary(bx, by, bz) {
                    let block = world.get(x + bx, y + by, z + bz);
                    if by >= 4 && is_liquid(block) {
                        return;
                    }
                    if by < 4 && !is_solid(block) && block != liquid {
                        return;
                    }
                }
            }
        }
    }
    for bx in 0..16 {
        for bz in 0..16 {
            for by in 0..8 {
                if inside(bx, by, bz) {
                    let block = if by >= 4 { Block::Air } else { liquid };
                    world.set(x + bx, y + by, z + bz, block);
                }
            }
        }
    }
    for bx in 0..16 {
        for bz in 0..16 {
            for by in 4..8 {
                if inside(bx, by, bz)
                    && world.get(x + bx, y + by - 1, z + bz) == Block::Dirt
                    && world.sky_light(x + bx, y + by, z + bz) > 0
                {
                    world.set(x + bx, y + by - 1, z + bz, Block::Grass);
                }
            }
        }
    }
    if liquid == Block::Lava {
        for bx in 0..16 {
            for bz in 0..16 {
                for by in 0..8 {
                    if boundary(bx, by, bz)
                        && (by < 4 || rand.next_int(2) != 0)
                        && is_solid(world.get(x + bx, y + by, z + bz))
                    {
                        world.set(x + bx, y + by, z + bz, Block::Stone);
                    }
                }
            }
        }
    }
}

/// `WorldGenDungeons`: a cobblestone room four blocks tall with up to two
/// chests and a spawner. Its ceiling at `y + 4` is left as found.
fn dungeon(world: &mut PopulationWorld, rand: &mut JavaRandom, x: i32, y: i32, z: i32) {
    const HEIGHT: i32 = 3;
    let rx = rand.next_int(2) as i32 + 2;
    let rz = rand.next_int(2) as i32 + 2;
    let mut openings = 0;
    for bx in x - rx - 1..=x + rx + 1 {
        for by in y - 1..=y + HEIGHT + 1 {
            for bz in z - rz - 1..=z + rz + 1 {
                let block = world.get(bx, by, bz);
                if (by == y - 1 || by == y + HEIGHT + 1) && !is_solid(block) {
                    return;
                }
                if (bx == x - rx - 1 || bx == x + rx + 1 || bz == z - rz - 1 || bz == z + rz + 1)
                    && by == y
                    && world.is_air(bx, by, bz)
                    && world.is_air(bx, by + 1, bz)
                {
                    openings += 1;
                }
            }
        }
    }
    if !(1..=5).contains(&openings) {
        return;
    }
    for bx in x - rx - 1..=x + rx + 1 {
        for by in (y - 1..=y + HEIGHT).rev() {
            for bz in z - rz - 1..=z + rz + 1 {
                let interior = bx != x - rx - 1
                    && by != y - 1
                    && bz != z - rz - 1
                    && bx != x + rx + 1
                    && by != y + HEIGHT + 1
                    && bz != z + rz + 1;
                if interior {
                    world.set(bx, by, bz, Block::Air);
                } else if by >= 0 && !is_solid(world.get(bx, by - 1, bz)) {
                    world.set(bx, by, bz, Block::Air);
                } else if is_solid(world.get(bx, by, bz)) {
                    let block = if by == y - 1 && rand.next_int(4) != 0 {
                        Block::MossyCobblestone
                    } else {
                        Block::Cobblestone
                    };
                    world.set(bx, by, bz, block);
                }
            }
        }
    }
    for _ in 0..2 {
        for _ in 0..3 {
            let bx = x + rand.next_int((rx * 2 + 1) as u32) as i32 - rx;
            let bz = z + rand.next_int((rz * 2 + 1) as u32) as i32 - rz;
            if !world.is_air(bx, y, bz) {
                continue;
            }
            let walls = [(bx - 1, bz), (bx + 1, bz), (bx, bz - 1), (bx, bz + 1)]
                .into_iter()
                .filter(|&(wx, wz)| is_solid(world.get(wx, y, wz)))
                .count();
            if walls == 1 {
                let chest = super::dungeon_loot::generate_dungeon_chest(rand);
                world.set_chest(bx, y, bz, chest);
                break;
            }
        }
    }
    let kind = match rand.next_int(4) {
        0 => crate::entity::mobs::MobType::Skeleton,
        1 | 2 => crate::entity::mobs::MobType::Zombie,
        _ => crate::entity::mobs::MobType::Spider,
    };
    world.set_spawner(x, y, z, kind);
}
