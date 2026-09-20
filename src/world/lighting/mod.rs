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
    /// Light on the four exterior faces, copied from loaded neighboring chunks.
    /// A full-sky fallback keeps isolated chunks renderable during startup.
    borders: [Box<[u8]>; 4],
    /// Light at the four diagonal exterior corners, copied from diagonal
    /// neighboring chunks for smooth vertex-light samples.
    corners: [Box<[u8]>; 4],
}

impl Skylight {
    pub fn from_chunk(chunk: &Chunk) -> Self {
        Self::from_chunk_with_neighbors(chunk, None, None, None, None)
    }

    pub fn from_chunk_with_neighbors(
        chunk: &Chunk,
        west: Option<&Chunk>,
        east: Option<&Chunk>,
        north: Option<&Chunk>,
        south: Option<&Chunk>,
    ) -> Self {
        Self::from_chunk_with_neighbors_and_corners(
            chunk, west, east, north, south, None, None, None, None,
        )
    }

    pub fn from_chunk_with_neighbors_and_corners(
        chunk: &Chunk,
        west: Option<&Chunk>,
        east: Option<&Chunk>,
        north: Option<&Chunk>,
        south: Option<&Chunk>,
        northwest: Option<&Chunk>,
        northeast: Option<&Chunk>,
        southwest: Option<&Chunk>,
        southeast: Option<&Chunk>,
    ) -> Self {
        Self::from_chunk_with_neighbors_and_corners_impl(
            chunk, west, east, north, south, northwest, northeast, southwest, southeast, None,
        )
    }

    fn from_chunk_with_neighbors_and_corners_impl(
        chunk: &Chunk,
        west: Option<&Chunk>,
        east: Option<&Chunk>,
        north: Option<&Chunk>,
        south: Option<&Chunk>,
        northwest: Option<&Chunk>,
        northeast: Option<&Chunk>,
        southwest: Option<&Chunk>,
        southeast: Option<&Chunk>,
        suppressed_edge: Option<usize>,
    ) -> Self {
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
            if west.is_none() && suppressed_edge != Some(0) {
                for z in 0..CHUNK_SIZE {
                    seed_sky_edge(chunk, &mut sky, &mut sky_queue, 0, y, z);
                }
            }
            if east.is_none() && suppressed_edge != Some(1) {
                for z in 0..CHUNK_SIZE {
                    seed_sky_edge(chunk, &mut sky, &mut sky_queue, CHUNK_SIZE - 1, y, z);
                }
            }
            if north.is_none() && suppressed_edge != Some(2) {
                for x in 0..CHUNK_SIZE {
                    seed_sky_edge(chunk, &mut sky, &mut sky_queue, x, y, 0);
                }
            }
            if south.is_none() && suppressed_edge != Some(3) {
                for x in 0..CHUNK_SIZE {
                    seed_sky_edge(chunk, &mut sky, &mut sky_queue, x, y, CHUNK_SIZE - 1);
                }
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

        let mut borders = std::array::from_fn(|_| vec![MAX_LIGHT; LIGHT_CELLS].into_boxed_slice());
        let mut corners = std::array::from_fn(|_| vec![MAX_LIGHT; CHUNK_HEIGHT].into_boxed_slice());
        let neighbors = [west, east, north, south];
        for (border_index, neighbor) in neighbors.into_iter().enumerate() {
            let Some(neighbor) = neighbor else {
                continue;
            };
            // Compute the neighbor without seeding its shared face as open
            // sky. Otherwise both chunks treat that face as an independent
            // light source, which leaks light through caves and produces a
            // visible seam.
            let neighbor_light = match border_index {
                0 => Self::from_chunk_with_neighbors_and_corners_impl(
                    neighbor,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(1),
                ),
                1 => Self::from_chunk_with_neighbors_and_corners_impl(
                    neighbor,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(0),
                ),
                2 => Self::from_chunk_with_neighbors_and_corners_impl(
                    neighbor,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(3),
                ),
                _ => Self::from_chunk_with_neighbors_and_corners_impl(
                    neighbor,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(2),
                ),
            };
            match border_index {
                0 => {
                    for y in 0..CHUNK_HEIGHT {
                        for z in 0..CHUNK_SIZE {
                            borders[0][Chunk::index(0, y, z)] =
                                neighbor_light.get(CHUNK_SIZE - 1, y, z).unwrap_or(0);
                            seed_from_neighbor(
                                chunk,
                                &mut sky,
                                &mut sky_queue,
                                0,
                                y,
                                z,
                                borders[0][Chunk::index(0, y, z)],
                            );
                        }
                    }
                }
                1 => {
                    for y in 0..CHUNK_HEIGHT {
                        for z in 0..CHUNK_SIZE {
                            borders[1][Chunk::index(CHUNK_SIZE - 1, y, z)] =
                                neighbor_light.get(0, y, z).unwrap_or(0);
                            seed_from_neighbor(
                                chunk,
                                &mut sky,
                                &mut sky_queue,
                                CHUNK_SIZE - 1,
                                y,
                                z,
                                borders[1][Chunk::index(CHUNK_SIZE - 1, y, z)],
                            );
                        }
                    }
                }
                2 => {
                    for y in 0..CHUNK_HEIGHT {
                        for x in 0..CHUNK_SIZE {
                            borders[2][Chunk::index(x, y, 0)] =
                                neighbor_light.get(x, y, CHUNK_SIZE - 1).unwrap_or(0);
                            seed_from_neighbor(
                                chunk,
                                &mut sky,
                                &mut sky_queue,
                                x,
                                y,
                                0,
                                borders[2][Chunk::index(x, y, 0)],
                            );
                        }
                    }
                }
                _ => {
                    for y in 0..CHUNK_HEIGHT {
                        for x in 0..CHUNK_SIZE {
                            borders[3][Chunk::index(x, y, CHUNK_SIZE - 1)] =
                                neighbor_light.get(x, y, 0).unwrap_or(0);
                            seed_from_neighbor(
                                chunk,
                                &mut sky,
                                &mut sky_queue,
                                x,
                                y,
                                CHUNK_SIZE - 1,
                                borders[3][Chunk::index(x, y, CHUNK_SIZE - 1)],
                            );
                        }
                    }
                }
            }
        }

        // A smooth-lit corner can sample a cell that is outside two faces at
        // once. Face borders cannot describe that cell, so retain the light
        // from the corresponding diagonal chunk explicitly.
        let diagonal_neighbors = [northwest, northeast, southwest, southeast];
        let diagonal_coords = [
            (CHUNK_SIZE - 1, CHUNK_SIZE - 1),
            (0, CHUNK_SIZE - 1),
            (CHUNK_SIZE - 1, 0),
            (0, 0),
        ];
        for (corner_index, diagonal) in diagonal_neighbors.into_iter().enumerate() {
            let Some(diagonal) = diagonal else {
                continue;
            };
            let diagonal_light = Self::from_chunk(diagonal);
            let (local_x, local_z) = diagonal_coords[corner_index];
            for y in 0..CHUNK_HEIGHT {
                corners[corner_index][y] = diagonal_light
                    .get(local_x * (CHUNK_SIZE - 1), y, local_z * (CHUNK_SIZE - 1))
                    .unwrap_or(0);
            }
        }
        propagate(chunk, &mut sky, &mut sky_queue);

        Self {
            sky,
            block,
            borders,
            corners,
        }
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

    pub fn get_extended(&self, x: i32, y: i32, z: i32) -> u8 {
        if y < 0 {
            return 0;
        }
        if y >= CHUNK_HEIGHT as i32 {
            return MAX_LIGHT;
        }
        if x >= 0 && x < CHUNK_SIZE as i32 && z >= 0 && z < CHUNK_SIZE as i32 {
            return self.get(x as usize, y as usize, z as usize).unwrap_or(0);
        }
        if x < 0 && (0..CHUNK_SIZE as i32).contains(&z) {
            return self.borders[0][Chunk::index(0, y as usize, z as usize)];
        }
        if x >= CHUNK_SIZE as i32 && (0..CHUNK_SIZE as i32).contains(&z) {
            return self.borders[1][Chunk::index(CHUNK_SIZE - 1, y as usize, z as usize)];
        }
        if z < 0 && (0..CHUNK_SIZE as i32).contains(&x) {
            return self.borders[2][Chunk::index(x as usize, y as usize, 0)];
        }
        if z >= CHUNK_SIZE as i32 && (0..CHUNK_SIZE as i32).contains(&x) {
            return self.borders[3][Chunk::index(x as usize, y as usize, CHUNK_SIZE - 1)];
        }
        if x < 0 && z < 0 {
            return self.corners[0][y as usize];
        }
        if x >= CHUNK_SIZE as i32 && z < 0 {
            return self.corners[1][y as usize];
        }
        if x < 0 && z >= CHUNK_SIZE as i32 {
            return self.corners[2][y as usize];
        }
        if x >= CHUNK_SIZE as i32 && z >= CHUNK_SIZE as i32 {
            return self.corners[3][y as usize];
        }
        MAX_LIGHT
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

fn seed_from_neighbor(
    chunk: &Chunk,
    sky: &mut [u8],
    queue: &mut VecDeque<(usize, usize, usize)>,
    x: usize,
    y: usize,
    z: usize,
    neighbor_level: u8,
) {
    let level = neighbor_level.saturating_sub(light_opacity(chunk.get(x, y, z).unwrap()).max(1));
    let index = Chunk::index(x, y, z);
    if level > sky[index] {
        sky[index] = level;
        queue.push_back((x, y, z));
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
