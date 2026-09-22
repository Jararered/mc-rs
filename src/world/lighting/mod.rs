use std::collections::VecDeque;

use crate::world::block::block::BlockId;
use crate::world::block::properties::is_crossed_plant;
use crate::world::block::properties::is_torch;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;

const MAX_LIGHT: u8 = 15;
const LIGHT_CELLS: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;

/// The two 4-bit light channels used by Beta: sunlight and emitted block light.
pub struct Skylight {
    sky: Box<[u8]>,
    block: Box<[u8]>,
    /// Sunlight and block light on the four exterior faces, copied from loaded
    /// neighboring chunks. A full-sky fallback keeps isolated chunks renderable
    /// during startup. The channels stay separate so the time of day can dim
    /// sunlight without dimming torches.
    border_sky: [Box<[u8]>; 4],
    border_block: [Box<[u8]>; 4],
    /// Light at the four diagonal exterior corners, copied from diagonal
    /// neighboring chunks for smooth vertex-light samples.
    corner_sky: [Box<[u8]>; 4],
    corner_block: [Box<[u8]>; 4],
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

    /// Propagate both light channels once across the complete 3×3 snapshot.
    /// A light level cannot travel sixteen cells, so sources beyond this ring
    /// cannot affect the center chunk or its one-cell meshing border.
    fn from_complete_neighborhood(chunks: [&Chunk; 9]) -> Self {
        const WIDTH: usize = CHUNK_SIZE * 3;
        const CELLS: usize = WIDTH * CHUNK_HEIGHT * WIDTH;
        let index = |x: usize, y: usize, z: usize| (y * WIDTH + z) * WIDTH + x;
        let block_at = |x: usize, y: usize, z: usize| {
            chunks[(z / CHUNK_SIZE) * 3 + x / CHUNK_SIZE]
                .get(x % CHUNK_SIZE, y, z % CHUNK_SIZE)
                .unwrap()
        };
        let mut sky = vec![0u8; CELLS];
        let mut block = vec![0u8; CELLS];
        let mut sky_queue = VecDeque::new();
        let mut block_queue = VecDeque::new();
        for z in 0..WIDTH {
            for x in 0..WIDTH {
                let mut incoming = MAX_LIGHT;
                for y in (0..CHUNK_HEIGHT).rev() {
                    let opacity = light_opacity(block_at(x, y, z));
                    let level = if incoming == MAX_LIGHT && opacity == 0 {
                        MAX_LIGHT
                    } else {
                        incoming.saturating_sub(opacity)
                    };
                    let i = index(x, y, z);
                    sky[i] = level;
                    if level > 0 {
                        sky_queue.push_back((x, y, z));
                    }
                    incoming = level;
                    let emission = light_emission(block_at(x, y, z));
                    if emission > 0 {
                        block[i] = emission;
                        block_queue.push_back((x, y, z));
                    }
                }
            }
        }
        let propagate = |light: &mut [u8], queue: &mut VecDeque<(usize, usize, usize)>| {
            while let Some((x, y, z)) = queue.pop_front() {
                let source = light[index(x, y, z)];
                if source <= 1 {
                    continue;
                }
                for (dx, dy, dz) in [
                    (1, 0, 0),
                    (-1, 0, 0),
                    (0, 1, 0),
                    (0, -1, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                ] {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let nz = z as i32 + dz;
                    if nx < 0
                        || nx >= WIDTH as i32
                        || ny < 0
                        || ny >= CHUNK_HEIGHT as i32
                        || nz < 0
                        || nz >= WIDTH as i32
                    {
                        continue;
                    }
                    let (nx, ny, nz) = (nx as usize, ny as usize, nz as usize);
                    let attenuation = light_opacity(block_at(nx, ny, nz)).max(1);
                    let candidate = source.saturating_sub(attenuation);
                    let i = index(nx, ny, nz);
                    if candidate > light[i] {
                        light[i] = candidate;
                        queue.push_back((nx, ny, nz));
                    }
                }
            }
        };
        propagate(&mut sky, &mut sky_queue);
        propagate(&mut block, &mut block_queue);

        let mut result = Self {
            sky: vec![0; LIGHT_CELLS].into_boxed_slice(),
            block: vec![0; LIGHT_CELLS].into_boxed_slice(),
            border_sky: std::array::from_fn(|_| vec![MAX_LIGHT; LIGHT_CELLS].into_boxed_slice()),
            border_block: std::array::from_fn(|_| vec![0; LIGHT_CELLS].into_boxed_slice()),
            corner_sky: std::array::from_fn(|_| vec![MAX_LIGHT; CHUNK_HEIGHT].into_boxed_slice()),
            corner_block: std::array::from_fn(|_| vec![0; CHUNK_HEIGHT].into_boxed_slice()),
        };
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let i = Chunk::index(x, y, z);
                    result.sky[i] = sky[index(x + CHUNK_SIZE, y, z + CHUNK_SIZE)];
                    result.block[i] = block[index(x + CHUNK_SIZE, y, z + CHUNK_SIZE)];
                }
                let west = Chunk::index(0, y, z);
                let east = Chunk::index(CHUNK_SIZE - 1, y, z);
                result.border_sky[0][west] = sky[index(CHUNK_SIZE - 1, y, z + CHUNK_SIZE)];
                result.border_block[0][west] = block[index(CHUNK_SIZE - 1, y, z + CHUNK_SIZE)];
                result.border_sky[1][east] = sky[index(CHUNK_SIZE * 2, y, z + CHUNK_SIZE)];
                result.border_block[1][east] = block[index(CHUNK_SIZE * 2, y, z + CHUNK_SIZE)];
            }
            for x in 0..CHUNK_SIZE {
                let north = Chunk::index(x, y, 0);
                let south = Chunk::index(x, y, CHUNK_SIZE - 1);
                result.border_sky[2][north] = sky[index(x + CHUNK_SIZE, y, CHUNK_SIZE - 1)];
                result.border_block[2][north] = block[index(x + CHUNK_SIZE, y, CHUNK_SIZE - 1)];
                result.border_sky[3][south] = sky[index(x + CHUNK_SIZE, y, CHUNK_SIZE * 2)];
                result.border_block[3][south] = block[index(x + CHUNK_SIZE, y, CHUNK_SIZE * 2)];
            }
            for (corner, (x, z)) in [
                (CHUNK_SIZE - 1, CHUNK_SIZE - 1),
                (CHUNK_SIZE * 2, CHUNK_SIZE - 1),
                (CHUNK_SIZE - 1, CHUNK_SIZE * 2),
                (CHUNK_SIZE * 2, CHUNK_SIZE * 2),
            ]
            .into_iter()
            .enumerate()
            {
                result.corner_sky[corner][y] = sky[index(x, y, z)];
                result.corner_block[corner][y] = block[index(x, y, z)];
            }
        }
        result
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
        if let (
            None,
            Some(west),
            Some(east),
            Some(north),
            Some(south),
            Some(northwest),
            Some(northeast),
            Some(southwest),
            Some(southeast),
        ) = (
            suppressed_edge,
            west,
            east,
            north,
            south,
            northwest,
            northeast,
            southwest,
            southeast,
        ) {
            return Self::from_complete_neighborhood([
                northwest, north, northeast, west, chunk, east, southwest, south, southeast,
            ]);
        }
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

        let mut border_sky =
            std::array::from_fn(|_| vec![MAX_LIGHT; LIGHT_CELLS].into_boxed_slice());
        let mut border_block = std::array::from_fn(|_| vec![0; LIGHT_CELLS].into_boxed_slice());
        let mut corner_sky =
            std::array::from_fn(|_| vec![MAX_LIGHT; CHUNK_HEIGHT].into_boxed_slice());
        let mut corner_block = std::array::from_fn(|_| vec![0; CHUNK_HEIGHT].into_boxed_slice());
        let neighbors = [west, east, north, south];
        for (border_index, neighbor) in neighbors.into_iter().enumerate() {
            let Some(neighbor) = neighbor else {
                continue;
            };
            // Compute the neighbor without seeding its shared face as open
            // sky. Otherwise both chunks treat that face as an independent
            // light source, which leaks light through caves and produces a
            // visible seam.
            let neighbor_light = if suppressed_edge.is_some() {
                // This is a one-ring neighbor's light. Its diagonal sources are
                // sampled without recursing into another neighborhood.
                Self::from_chunk(neighbor)
            } else {
                match border_index {
                    0 => Self::from_chunk_with_neighbors_and_corners_impl(
                        neighbor,
                        None,
                        None,
                        northwest,
                        southwest,
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
                        northeast,
                        southeast,
                        None,
                        None,
                        None,
                        None,
                        Some(0),
                    ),
                    2 => Self::from_chunk_with_neighbors_and_corners_impl(
                        neighbor,
                        northwest,
                        northeast,
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
                        southwest,
                        southeast,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(2),
                    ),
                }
            };
            match border_index {
                0 => {
                    for y in 0..CHUNK_HEIGHT {
                        for z in 0..CHUNK_SIZE {
                            let (sky_level, block_level) =
                                neighbor_channels(&neighbor_light, CHUNK_SIZE - 1, y, z);
                            let index = Chunk::index(0, y, z);
                            border_sky[0][index] = sky_level;
                            border_block[0][index] = block_level;
                            seed_from_neighbor(chunk, &mut sky, &mut sky_queue, 0, y, z, sky_level);
                            seed_from_neighbor(
                                chunk,
                                &mut block,
                                &mut block_queue,
                                0,
                                y,
                                z,
                                block_level,
                            );
                        }
                    }
                }
                1 => {
                    for y in 0..CHUNK_HEIGHT {
                        for z in 0..CHUNK_SIZE {
                            let (sky_level, block_level) =
                                neighbor_channels(&neighbor_light, 0, y, z);
                            let index = Chunk::index(CHUNK_SIZE - 1, y, z);
                            border_sky[1][index] = sky_level;
                            border_block[1][index] = block_level;
                            seed_from_neighbor(
                                chunk,
                                &mut sky,
                                &mut sky_queue,
                                CHUNK_SIZE - 1,
                                y,
                                z,
                                sky_level,
                            );
                            seed_from_neighbor(
                                chunk,
                                &mut block,
                                &mut block_queue,
                                CHUNK_SIZE - 1,
                                y,
                                z,
                                block_level,
                            );
                        }
                    }
                }
                2 => {
                    for y in 0..CHUNK_HEIGHT {
                        for x in 0..CHUNK_SIZE {
                            let (sky_level, block_level) =
                                neighbor_channels(&neighbor_light, x, y, CHUNK_SIZE - 1);
                            let index = Chunk::index(x, y, 0);
                            border_sky[2][index] = sky_level;
                            border_block[2][index] = block_level;
                            seed_from_neighbor(chunk, &mut sky, &mut sky_queue, x, y, 0, sky_level);
                            seed_from_neighbor(
                                chunk,
                                &mut block,
                                &mut block_queue,
                                x,
                                y,
                                0,
                                block_level,
                            );
                        }
                    }
                }
                _ => {
                    for y in 0..CHUNK_HEIGHT {
                        for x in 0..CHUNK_SIZE {
                            let (sky_level, block_level) =
                                neighbor_channels(&neighbor_light, x, y, 0);
                            let index = Chunk::index(x, y, CHUNK_SIZE - 1);
                            border_sky[3][index] = sky_level;
                            border_block[3][index] = block_level;
                            seed_from_neighbor(
                                chunk,
                                &mut sky,
                                &mut sky_queue,
                                x,
                                y,
                                CHUNK_SIZE - 1,
                                sky_level,
                            );
                            seed_from_neighbor(
                                chunk,
                                &mut block,
                                &mut block_queue,
                                x,
                                y,
                                CHUNK_SIZE - 1,
                                block_level,
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
                let (sky_level, block_level) = neighbor_channels(
                    &diagonal_light,
                    local_x * (CHUNK_SIZE - 1),
                    y,
                    local_z * (CHUNK_SIZE - 1),
                );
                corner_sky[corner_index][y] = sky_level;
                corner_block[corner_index][y] = block_level;
            }
        }
        propagate(chunk, &mut sky, &mut sky_queue);
        propagate(chunk, &mut block, &mut block_queue);

        Self {
            sky,
            block,
            border_sky,
            border_block,
            corner_sky,
            corner_block,
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
        self.light_at(x, y, z, 0)
    }

    /// Combined light after `Chunk.getBlockLightValue`. `skylight_subtracted`
    /// is the time-of-day penalty from `World.calculateSkylightSubtracted`.
    pub fn light_at(&self, x: i32, y: i32, z: i32, skylight_subtracted: u8) -> u8 {
        if y < 0 {
            return 0;
        }
        if y >= CHUNK_HEIGHT as i32 {
            return combined_light(MAX_LIGHT, 0, skylight_subtracted);
        }
        let (sky, block) = if x >= 0 && x < CHUNK_SIZE as i32 && z >= 0 && z < CHUNK_SIZE as i32 {
            let index = Chunk::index(x as usize, y as usize, z as usize);
            (self.sky[index], self.block[index])
        } else if x < 0 && (0..CHUNK_SIZE as i32).contains(&z) {
            let index = Chunk::index(0, y as usize, z as usize);
            (self.border_sky[0][index], self.border_block[0][index])
        } else if x >= CHUNK_SIZE as i32 && (0..CHUNK_SIZE as i32).contains(&z) {
            let index = Chunk::index(CHUNK_SIZE - 1, y as usize, z as usize);
            (self.border_sky[1][index], self.border_block[1][index])
        } else if z < 0 && (0..CHUNK_SIZE as i32).contains(&x) {
            let index = Chunk::index(x as usize, y as usize, 0);
            (self.border_sky[2][index], self.border_block[2][index])
        } else if z >= CHUNK_SIZE as i32 && (0..CHUNK_SIZE as i32).contains(&x) {
            let index = Chunk::index(x as usize, y as usize, CHUNK_SIZE - 1);
            (self.border_sky[3][index], self.border_block[3][index])
        } else if x < 0 && z < 0 {
            (
                self.corner_sky[0][y as usize],
                self.corner_block[0][y as usize],
            )
        } else if x >= CHUNK_SIZE as i32 && z < 0 {
            (
                self.corner_sky[1][y as usize],
                self.corner_block[1][y as usize],
            )
        } else if x < 0 && z >= CHUNK_SIZE as i32 {
            (
                self.corner_sky[2][y as usize],
                self.corner_block[2][y as usize],
            )
        } else if x >= CHUNK_SIZE as i32 && z >= CHUNK_SIZE as i32 {
            (
                self.corner_sky[3][y as usize],
                self.corner_block[3][y as usize],
            )
        } else {
            (MAX_LIGHT, 0)
        };
        combined_light(sky, block, skylight_subtracted)
    }
}

fn neighbor_channels(light: &Skylight, x: usize, y: usize, z: usize) -> (u8, u8) {
    (
        light.sky(x, y, z).unwrap_or(0),
        light.block(x, y, z).unwrap_or(0),
    )
}

/// `Chunk.getBlockLightValue`: sunlight loses `skylight_subtracted` levels as
/// the day ends, then the brighter of that and the block light is kept.
pub fn combined_light(sky: u8, block: u8, skylight_subtracted: u8) -> u8 {
    sky.saturating_sub(skylight_subtracted.min(MAX_LIGHT))
        .max(block.min(MAX_LIGHT))
}

/// Vertex brightness for a light level. This is Beta's
/// `WorldProvider.lightBrightnessTable`, the ambient value meshes bake in.
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
        BlockId::SnowLayer | BlockId::Cactus => 0,
        block if is_torch(block) || is_crossed_plant(block) => 0,
        _ => 15,
    }
}

pub fn light_emission(block: BlockId) -> u8 {
    match block {
        BlockId::Glowstone | BlockId::JackOLantern => 15,
        BlockId::Lava | BlockId::FlowingLava => 15,
        block if is_torch(block) => 15,
        block if block.is_lit_furnace() => 13,
        BlockId::LitRedstoneOre => 9,
        _ => 0,
    }
}
