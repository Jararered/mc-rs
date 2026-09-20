use std::collections::VecDeque;

use crate::world::block::block::BlockId;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;

const MAX_LIGHT: u8 = 15;
const LIGHT_CELLS: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;

/// The two 4-bit light channels used by Beta: sunlight and emitted block light.
pub struct Skylight {
    sky: Box<[u8]>,
    block: Box<[u8]>,
}

impl Skylight {
    pub fn from_chunk(chunk: &Chunk) -> Self {
        let mut sky = vec![0; LIGHT_CELLS].into_boxed_slice();
        let mut block = vec![0; LIGHT_CELLS].into_boxed_slice();
        let mut sky_queue = VecDeque::new();
        let mut block_queue = VecDeque::new();

        // Seed direct sunlight from above. Open chunk edges are also seeded so
        // caves and overhangs do not get an artificial black streaming seam.
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let mut incoming = MAX_LIGHT;
                for y in (0..CHUNK_HEIGHT).rev() {
                    let opacity = light_opacity(chunk.get(x, y, z).unwrap());
                    let level = if incoming == MAX_LIGHT && opacity == 0 {
                        MAX_LIGHT
                    } else {
                        incoming.saturating_sub(opacity)
                    };
                    let index = Chunk::index(x, y, z);
                    sky[index] = level;
                    if level > 0 {
                        sky_queue.push_back((x, y, z));
                    }
                    incoming = level;
                }
            }
        }
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE {
                seed_sky_edge(chunk, &mut sky, &mut sky_queue, 0, y, z);
                seed_sky_edge(chunk, &mut sky, &mut sky_queue, CHUNK_SIZE - 1, y, z);
            }
            for x in 0..CHUNK_SIZE {
                seed_sky_edge(chunk, &mut sky, &mut sky_queue, x, y, 0);
                seed_sky_edge(chunk, &mut sky, &mut sky_queue, x, y, CHUNK_SIZE - 1);
            }
        }

        // Beta block emitters currently represented by this project.
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let level = light_emission(chunk.get(x, y, z).unwrap());
                    if level > 0 {
                        let index = Chunk::index(x, y, z);
                        block[index] = level;
                        block_queue.push_back((x, y, z));
                    }
                }
            }
        }

        propagate(chunk, &mut sky, &mut sky_queue);
        propagate(chunk, &mut block, &mut block_queue);

        Self { sky, block }
    }

    /// Combined Beta light value at a block, after taking the brighter channel.
    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        self.light(x, y, z)
    }

    pub fn light(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return None;
        }
        let index = Chunk::index(x, y, z);
        Some(self.sky[index].max(self.block[index]))
    }

    pub fn sky(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return None;
        }
        Some(self.sky[Chunk::index(x, y, z)])
    }

    pub fn block(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return None;
        }
        Some(self.block[Chunk::index(x, y, z)])
    }
}

/// Beta's `WorldProvider.lightBrightnessTable`.
pub fn beta_brightness(level: u8) -> f32 {
    let level = level.min(MAX_LIGHT) as f32;
    let darkness = 1.0 - level / 15.0;
    let base = 0.05;
    (1.0 - darkness) / (darkness * 3.0 + 1.0) * (1.0 - base) + base
}

fn seed_sky_edge(
    chunk: &Chunk,
    sky: &mut [u8],
    queue: &mut VecDeque<(usize, usize, usize)>,
    x: usize,
    y: usize,
    z: usize,
) {
    if light_opacity(chunk.get(x, y, z).unwrap()) == 0 {
        let index = Chunk::index(x, y, z);
        if sky[index] < MAX_LIGHT {
            sky[index] = MAX_LIGHT;
            queue.push_back((x, y, z));
        }
    }
}

fn propagate(chunk: &Chunk, light: &mut [u8], queue: &mut VecDeque<(usize, usize, usize)>) {
    const DIRECTIONS: [[i32; 3]; 6] = [
        [1, 0, 0],
        [-1, 0, 0],
        [0, 1, 0],
        [0, -1, 0],
        [0, 0, 1],
        [0, 0, -1],
    ];

    while let Some((x, y, z)) = queue.pop_front() {
        let source = light[Chunk::index(x, y, z)];
        if source <= 1 {
            continue;
        }
        for direction in DIRECTIONS {
            let nx = x as i32 + direction[0];
            let ny = y as i32 + direction[1];
            let nz = z as i32 + direction[2];
            if nx < 0
                || nx >= CHUNK_SIZE as i32
                || ny < 0
                || ny >= CHUNK_HEIGHT as i32
                || nz < 0
                || nz >= CHUNK_SIZE as i32
            {
                continue;
            }
            let (nx, ny, nz) = (nx as usize, ny as usize, nz as usize);
            let attenuation = light_opacity(chunk.get(nx, ny, nz).unwrap()).max(1);
            let candidate = source.saturating_sub(attenuation);
            let index = Chunk::index(nx, ny, nz);
            if candidate > light[index] {
                light[index] = candidate;
                queue.push_back((nx, ny, nz));
            }
        }
    }
}

/// Beta `Block.lightOpacity`, expressed in light levels rather than its old
/// internal 0..255 table.
pub fn light_opacity(block: BlockId) -> u8 {
    match block {
        BlockId::Air => 0,
        BlockId::Water | BlockId::Ice => 3,
        BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves => 1,
        BlockId::Snow => 0,
        _ => 15,
    }
}

pub fn light_emission(block: BlockId) -> u8 {
    match block {
        BlockId::Glowstone | BlockId::JackOLantern => 15,
        BlockId::LitFurnace => 13,
        BlockId::LitRedstoneOre => 9,
        _ => 0,
    }
}
