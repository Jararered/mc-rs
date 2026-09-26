//! Beta `ChunkProviderGenerate.populate` underground passes.
use crate::block::id::Id;
use crate::random::JavaRandom;
use crate::world::chest::Chest;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use std::collections::HashMap;

const PI: f32 = 3.1415927;

pub(super) fn source_random(seed: u64, source: ChunkPosition) -> JavaRandom {
    let mut random = JavaRandom::new(seed);
    let salt_x = random.next_long() / 2 * 2 + 1;
    let salt_z = random.next_long() / 2 * 2 + 1;
    let chunk_seed = (source.x as i64)
        .wrapping_mul(salt_x)
        .wrapping_add((source.z as i64).wrapping_mul(salt_z))
        ^ seed as i64;
    JavaRandom::new(chunk_seed as u64)
}

struct WorldView<'a> {
    chunk: &'a mut Chunk,
    target: ChunkPosition,
    remote_chunk: &'a dyn Fn(ChunkPosition) -> Chunk,
    cached: HashMap<ChunkPosition, Chunk>,
}

impl WorldView<'_> {
    fn get(&mut self, x: i32, y: i32, z: i32) -> Id {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return Id::Air;
        }
        let pos = ChunkPosition {
            x: x.div_euclid(16),
            z: z.div_euclid(16),
        };
        let lx = x.rem_euclid(16) as usize;
        let lz = z.rem_euclid(16) as usize;
        if pos == self.target {
            return self.chunk.get(lx, y as usize, lz).unwrap();
        }
        let cached = self
            .cached
            .entry(pos)
            .or_insert_with(|| (self.remote_chunk)(pos));
        cached.get(lx, y as usize, lz).unwrap()
    }

    fn set(&mut self, x: i32, y: i32, z: i32, block: Id) {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return;
        }
        let pos = ChunkPosition {
            x: x.div_euclid(16),
            z: z.div_euclid(16),
        };
        if pos == self.target {
            self.chunk.set(
                x.rem_euclid(16) as usize,
                y as usize,
                z.rem_euclid(16) as usize,
                block,
            );
        }
    }

    fn set_chest(&mut self, x: i32, y: i32, z: i32, chest: Chest) {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return;
        }
        let pos = ChunkPosition {
            x: x.div_euclid(16),
            z: z.div_euclid(16),
        };
        if pos != self.target {
            return;
        }
        let local_x = x.rem_euclid(16) as usize;
        let local_y = y as usize;
        let local_z = z.rem_euclid(16) as usize;
        self.chunk.set(local_x, local_y, local_z, Id::Chest);
        self.chunk
            .insert_chest(Chunk::index(local_x, local_y, local_z), chest);
    }
}

pub(super) fn populate(
    chunk: &mut Chunk,
    target: ChunkPosition,
    seed: u64,
    remote_chunk: &dyn Fn(ChunkPosition) -> Chunk,
) -> HashMap<ChunkPosition, JavaRandom> {
    let mut world = WorldView {
        chunk,
        target,
        remote_chunk,
        cached: Default::default(),
    };
    let mut after = HashMap::new();
    for sx in target.x - 1..=target.x + 1 {
        for sz in target.z - 1..=target.z + 1 {
            let source = ChunkPosition { x: sx, z: sz };
            let mut random = source_random(seed, source);
            populate_source(&mut world, source, &mut random);
            after.insert(source, random);
        }
    }
    after
}

fn populate_source(world: &mut WorldView<'_>, source: ChunkPosition, random: &mut JavaRandom) {
    let ox = source.x * CHUNK_SIZE as i32;
    let oz = source.z * CHUNK_SIZE as i32;
    if random.next_int(4) == 0 {
        let x = ox + random.next_int(16) as i32 + 8;
        let y = random.next_int(128) as i32;
        let z = oz + random.next_int(16) as i32 + 8;
        lake(world, random, x, y, z, Id::Water);
    }
    if random.next_int(8) == 0 {
        let x = ox + random.next_int(16) as i32 + 8;
        let y_bound = random.next_int(120) + 8;
        let y = random.next_int(y_bound) as i32;
        let z = oz + random.next_int(16) as i32 + 8;
        if y < 64 || random.next_int(10) == 0 {
            lake(world, random, x, y, z, Id::Lava);
        }
    }
    for _ in 0..8 {
        let x = ox + random.next_int(16) as i32 + 8;
        let y = random.next_int(128) as i32;
        let z = oz + random.next_int(16) as i32 + 8;
        dungeon(world, random, x, y, z);
    }
    for _ in 0..10 {
        let x = ox + random.next_int(16) as i32;
        let y = random.next_int(128) as i32;
        let z = oz + random.next_int(16) as i32;
        if matches!(world.get(x, y, z), Id::Water | Id::FlowingWater) {
            vein(world, random, x, y, z, 32, Id::Clay, Id::Sand);
        }
    }
    const ORES: [(u32, u32, u32, Id); 8] = [
        (20, 128, 32, Id::Dirt),
        (10, 128, 32, Id::Gravel),
        (20, 128, 16, Id::CoalOre),
        (20, 64, 8, Id::IronOre),
        (2, 32, 8, Id::GoldOre),
        (8, 16, 7, Id::RedstoneOre),
        (1, 16, 7, Id::DiamondOre),
        (1, 0, 6, Id::LapisOre),
    ];
    for (count, max_y, size, block) in ORES {
        for _ in 0..count {
            let x = ox + random.next_int(16) as i32;
            let y = if block == Id::LapisOre {
                random.next_int(16) as i32 + random.next_int(16) as i32
            } else {
                random.next_int(max_y) as i32
            };
            let z = oz + random.next_int(16) as i32;
            vein(world, random, x, y, z, size, block, Id::Stone);
        }
    }
}

fn vein(
    world: &mut WorldView<'_>,
    random: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
    size: u32,
    block: Id,
    replace: Id,
) {
    let angle = random.next_float() * PI;
    let dx = (angle.sin() * size as f32 / 8.0) as f64;
    let dz = (angle.cos() * size as f32 / 8.0) as f64;
    let x0 = (x + 8) as f64 + dx;
    let x1 = (x + 8) as f64 - dx;
    let z0 = (z + 8) as f64 + dz;
    let z1 = (z + 8) as f64 - dz;
    let y0 = (y + random.next_int(3) as i32 + 2) as f64;
    let y1 = (y + random.next_int(3) as i32 + 2) as f64;
    for i in 0..=size {
        let t = i as f64 / size as f64;
        let cx = x0 + (x1 - x0) * t;
        let cy = y0 + (y1 - y0) * t;
        let cz = z0 + (z1 - z0) * t;
        let jitter = random.next_double() * size as f64 / 16.0;
        let diameter = ((i as f32 * PI / size as f32).sin() + 1.0) as f64 * jitter + 1.0;
        let r = diameter / 2.0;
        for bx in (cx - r).floor() as i32..=(cx + r).floor() as i32 {
            let nx = (bx as f64 + 0.5 - cx) / r;
            if nx * nx >= 1.0 {
                continue;
            }
            for by in (cy - r).floor() as i32..=(cy + r).floor() as i32 {
                let ny = (by as f64 + 0.5 - cy) / r;
                if nx * nx + ny * ny >= 1.0 {
                    continue;
                }
                for bz in (cz - r).floor() as i32..=(cz + r).floor() as i32 {
                    let nz = (bz as f64 + 0.5 - cz) / r;
                    if nx * nx + ny * ny + nz * nz < 1.0 && world.get(bx, by, bz) == replace {
                        world.set(bx, by, bz, block);
                    }
                }
            }
        }
    }
}

fn lake(
    world: &mut WorldView<'_>,
    random: &mut JavaRandom,
    x: i32,
    mut y: i32,
    z: i32,
    liquid: Id,
) {
    let x = x - 8;
    let z = z - 8;
    while y > 0 && world.get(x, y, z) == Id::Air {
        y -= 1;
    }
    y -= 4;
    let mut shape = [false; 2048];
    let ellipsoids = random.next_int(4) + 4;
    for _ in 0..ellipsoids {
        let rx = random.next_double() * 6.0 + 3.0;
        let ry = random.next_double() * 4.0 + 2.0;
        let rz = random.next_double() * 6.0 + 3.0;
        let cx = random.next_double() * (16.0 - rx - 2.0) + 1.0 + rx / 2.0;
        let cy = random.next_double() * (8.0 - ry - 4.0) + 2.0 + ry / 2.0;
        let cz = random.next_double() * (16.0 - rz - 2.0) + 1.0 + rz / 2.0;
        for bx in 1..15 {
            for bz in 1..15 {
                for by in 1..7 {
                    let dx = (bx as f64 - cx) / (rx / 2.0);
                    let dy = (by as f64 - cy) / (ry / 2.0);
                    let dz = (bz as f64 - cz) / (rz / 2.0);
                    if dx * dx + dy * dy + dz * dz < 1.0 {
                        shape[((bx * 16 + bz) * 8 + by) as usize] = true;
                    }
                }
            }
        }
    }
    let inside = |bx: i32, by: i32, bz: i32| -> bool {
        (0..16).contains(&bx)
            && (0..8).contains(&by)
            && (0..16).contains(&bz)
            && shape[((bx * 16 + bz) * 8 + by) as usize]
    };
    for bx in 0..16 {
        for bz in 0..16 {
            for by in 0..8 {
                let boundary = !inside(bx, by, bz)
                    && (inside(bx + 1, by, bz)
                        || inside(bx - 1, by, bz)
                        || inside(bx, by + 1, bz)
                        || inside(bx, by - 1, bz)
                        || inside(bx, by, bz + 1)
                        || inside(bx, by, bz - 1));
                if boundary {
                    let old = world.get(x + bx, y + by, z + bz);
                    if by >= 4
                        && matches!(
                            old,
                            Id::Water | Id::FlowingWater | Id::Lava | Id::FlowingLava
                        )
                    {
                        return;
                    }
                    if by < 4 && !solid(old) && old != liquid {
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
                    world.set(
                        x + bx,
                        y + by,
                        z + bz,
                        if by >= 4 { Id::Air } else { liquid },
                    );
                }
            }
        }
    }
    for bx in 0..16 {
        for bz in 0..16 {
            for by in 4..8 {
                if inside(bx, by, bz) && world.get(x + bx, y + by - 1, z + bz) == Id::Dirt {
                    // Sky exposure approximates Beta's saved skylight during population.
                    if (y + by..CHUNK_HEIGHT as i32)
                        .all(|above| world.get(x + bx, above, z + bz) == Id::Air)
                    {
                        world.set(x + bx, y + by - 1, z + bz, Id::Grass);
                    }
                }
            }
        }
    }
    if liquid == Id::Lava {
        for bx in 0..16 {
            for bz in 0..16 {
                for by in 0..8 {
                    let boundary = !inside(bx, by, bz)
                        && (inside(bx + 1, by, bz)
                            || inside(bx - 1, by, bz)
                            || inside(bx, by + 1, bz)
                            || inside(bx, by - 1, bz)
                            || inside(bx, by, bz + 1)
                            || inside(bx, by, bz - 1));
                    if boundary
                        && (by < 4 || random.next_int(2) != 0)
                        && solid(world.get(x + bx, y + by, z + bz))
                    {
                        world.set(x + bx, y + by, z + bz, Id::Stone);
                    }
                }
            }
        }
    }
}

fn solid(block: Id) -> bool {
    !matches!(
        block,
        Id::Air | Id::Water | Id::FlowingWater | Id::Lava | Id::FlowingLava
    )
}

fn dungeon(world: &mut WorldView<'_>, random: &mut JavaRandom, x: i32, y: i32, z: i32) {
    let rx = random.next_int(2) as i32 + 2;
    let rz = random.next_int(2) as i32 + 2;
    let mut openings = 0;
    for bx in x - rx - 1..=x + rx + 1 {
        for by in y - 1..=y + 4 {
            for bz in z - rz - 1..=z + rz + 1 {
                let block = world.get(bx, by, bz);
                if (by == y - 1 || by == y + 4) && !solid(block) {
                    return;
                }
                if (bx == x - rx - 1 || bx == x + rx + 1 || bz == z - rz - 1 || bz == z + rz + 1)
                    && by == y
                    && block == Id::Air
                    && world.get(bx, by + 1, bz) == Id::Air
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
        for by in (y - 1..=y + 3).rev() {
            for bz in z - rz - 1..=z + rz + 1 {
                let edge = bx == x - rx - 1
                    || bx == x + rx + 1
                    || bz == z - rz - 1
                    || bz == z + rz + 1
                    || by == y - 1
                    || by == y + 3;
                if !edge || (by >= 0 && !solid(world.get(bx, by - 1, bz))) {
                    world.set(bx, by, bz, Id::Air);
                } else if solid(world.get(bx, by, bz)) {
                    let block = if by == y - 1 && random.next_int(4) != 0 {
                        Id::MossyCobblestone
                    } else {
                        Id::Cobblestone
                    };
                    world.set(bx, by, bz, block);
                }
            }
        }
    }
    for _ in 0..2 {
        for _ in 0..3 {
            let bx = x + random.next_int((rx * 2 + 1) as u32) as i32 - rx;
            let bz = z + random.next_int((rz * 2 + 1) as u32) as i32 - rz;
            if world.get(bx, y, bz) != Id::Air {
                continue;
            }
            let neighbors = [
                world.get(bx - 1, y, bz),
                world.get(bx + 1, y, bz),
                world.get(bx, y, bz - 1),
                world.get(bx, y, bz + 1),
            ];
            if neighbors.into_iter().filter(|b| solid(*b)).count() == 1 {
                let chest = super::dungeon_loot::generate_dungeon_chest(random);
                world.set_chest(bx, y, bz, chest);
                break;
            }
        }
    }
    world.set(x, y, z, Id::MobSpawner);
    let _ = random.next_int(4);
}
