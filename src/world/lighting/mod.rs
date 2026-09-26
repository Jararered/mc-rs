use std::collections::HashMap;
use std::collections::VecDeque;
use std::hash::Hasher;
use std::sync::Arc;
use std::sync::OnceLock;

use bevy::prelude::Resource;

use crate::block::definition::BlockProperties;
use crate::block::id::Id;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;
use crate::world::chunk::SECTIONS_PER_CHUNK;
use crate::world::chunk::WorldChunks;

const MAX_LIGHT: u8 = 15;
const LIGHT_CELLS: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;
/// Stored width: the chunk plus the one-cell ring that meshing samples.
const PADDED: usize = CHUNK_SIZE + 2;
const PADDED_LAYER: usize = PADDED * PADDED;
const PADDED_CELLS: usize = PADDED_LAYER * CHUNK_HEIGHT;
/// Full sunlight and no block light, the fallback for a missing neighbor.
const OPEN_SKY: u8 = pack(MAX_LIGHT, 0);

/// The two 4-bit light channels used by Beta: sunlight and emitted block light.
///
/// Each cell packs sky light in the high nibble and block light in the low
/// nibble. The channels stay separate so the time of day can dim sunlight
/// without dimming torches; that happens in the block shader, not here.
pub struct Skylight {
    /// The chunk plus a one-cell ring copied from its loaded neighbors, in
    /// [`padded_index`] order. Face culling, smooth lighting, and ambient
    /// occlusion sample that ring. A full-sky fallback keeps isolated chunks
    /// renderable during startup.
    cells: Box<[u8]>,
}

pub const fn pack(sky: u8, block: u8) -> u8 {
    (sky << 4) | (block & 0x0f)
}

pub const fn unpack(cell: u8) -> (u8, u8) {
    (cell >> 4, cell & 0x0f)
}

const fn padded_index(x: i32, y: usize, z: i32) -> usize {
    (y * PADDED + (z + 1) as usize) * PADDED + (x + 1) as usize
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
        const LAYER: usize = WIDTH * WIDTH;
        const CELLS: usize = LAYER * CHUNK_HEIGHT;
        let tables = light_tables();

        // Resolve every cell's opacity once from the raw block bytes, so the
        // flood below never goes back through chunk lookups or `Id` decoding.
        let mut opacity = vec![0u8; CELLS];
        let mut block = vec![0u8; CELLS];
        let mut block_queue = VecDeque::new();
        for (slot, chunk) in chunks.iter().enumerate() {
            let origin_x = slot % 3 * CHUNK_SIZE;
            let origin_z = slot / 3 * CHUNK_SIZE;
            let raw = chunk.raw_blocks();
            for y in 0..CHUNK_HEIGHT {
                for z in 0..CHUNK_SIZE {
                    let source = Chunk::index(0, y, z);
                    let row = y * LAYER + (origin_z + z) * WIDTH + origin_x;
                    for x in 0..CHUNK_SIZE {
                        let id = usize::from(raw[source + x]);
                        opacity[row + x] = tables.opacity[id];
                        let emission = tables.emission[id];
                        if emission > 0 {
                            block[row + x] = emission;
                            block_queue.push_back((row + x) as u32);
                        }
                    }
                }
            }
        }

        // Direct sunlight falls straight down each column.
        let mut sky = vec![0u8; CELLS];
        for column in 0..LAYER {
            let mut incoming = MAX_LIGHT;
            for y in (0..CHUNK_HEIGHT).rev() {
                let i = y * LAYER + column;
                let cell_opacity = opacity[i];
                let level = if incoming == MAX_LIGHT && cell_opacity == 0 {
                    MAX_LIGHT
                } else {
                    incoming.saturating_sub(cell_opacity)
                };
                sky[i] = level;
                incoming = level;
            }
        }

        // Only enqueue sunlit cells that can brighten a neighbor. Open air at
        // full light surrounded by full light is most of the volume, and
        // flooding from it can never change anything. Relaxation reaches the
        // same fixed point in any order, so skipping those cells is exact.
        let mut sky_queue = VecDeque::new();
        for (i, &level) in sky.iter().enumerate() {
            if level > 1
                && neighbors::<WIDTH, LAYER>(i)
                    .any(|n| level.saturating_sub(opacity[n].max(1)) > sky[n])
            {
                sky_queue.push_back(i as u32);
            }
        }
        flood::<WIDTH, LAYER>(&mut sky, &opacity, sky_queue);
        flood::<WIDTH, LAYER>(&mut block, &opacity, block_queue);

        let mut cells = vec![0u8; PADDED_CELLS].into_boxed_slice();
        let ring_origin = CHUNK_SIZE - 1;
        for y in 0..CHUNK_HEIGHT {
            for z in 0..PADDED {
                let source = y * LAYER + (ring_origin + z) * WIDTH + ring_origin;
                let target = (y * PADDED + z) * PADDED;
                for x in 0..PADDED {
                    cells[target + x] = pack(sky[source + x], block[source + x]);
                }
            }
        }
        Self { cells }
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
        let light_table = crate::block::definition::properties_table();

        // Seed direct sunlight from above. Open chunk edges are also seeded so
        // caves and overhangs do not get an artificial black streaming seam.
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let mut incoming = MAX_LIGHT;
                for y in (0..CHUNK_HEIGHT).rev() {
                    let opacity =
                        light_opacity_from_table(light_table, chunk.get(x, y, z).unwrap());
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
                    seed_sky_edge(chunk, light_table, &mut sky, &mut sky_queue, 0, y, z);
                }
            }
            if east.is_none() && suppressed_edge != Some(1) {
                for z in 0..CHUNK_SIZE {
                    seed_sky_edge(
                        chunk,
                        light_table,
                        &mut sky,
                        &mut sky_queue,
                        CHUNK_SIZE - 1,
                        y,
                        z,
                    );
                }
            }
            if north.is_none() && suppressed_edge != Some(2) {
                for x in 0..CHUNK_SIZE {
                    seed_sky_edge(chunk, light_table, &mut sky, &mut sky_queue, x, y, 0);
                }
            }
            if south.is_none() && suppressed_edge != Some(3) {
                for x in 0..CHUNK_SIZE {
                    seed_sky_edge(
                        chunk,
                        light_table,
                        &mut sky,
                        &mut sky_queue,
                        x,
                        y,
                        CHUNK_SIZE - 1,
                    );
                }
            }
        }

        // Beta block emitters currently represented by this project.
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let level = light_emission_from_table(light_table, chunk.get(x, y, z).unwrap());
                    if level > 0 {
                        let index = Chunk::index(x, y, z);
                        block[index] = level;
                        block_queue.push_back((x, y, z));
                    }
                }
            }
        }

        propagate(chunk, light_table, &mut sky, &mut sky_queue);
        propagate(chunk, light_table, &mut block, &mut block_queue);

        let mut cells = vec![OPEN_SKY; PADDED_CELLS].into_boxed_slice();
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
            // The neighbor-side cell next to each edge cell of this chunk, and
            // the ring position that stores it.
            for y in 0..CHUNK_HEIGHT {
                for along in 0..CHUNK_SIZE {
                    let (local, neighbor_cell, ring) = match border_index {
                        0 => ((0, along), (CHUNK_SIZE - 1, along), (-1, along as i32)),
                        1 => (
                            (CHUNK_SIZE - 1, along),
                            (0, along),
                            (CHUNK_SIZE as i32, along as i32),
                        ),
                        2 => ((along, 0), (along, CHUNK_SIZE - 1), (along as i32, -1)),
                        _ => (
                            (along, CHUNK_SIZE - 1),
                            (along, 0),
                            (along as i32, CHUNK_SIZE as i32),
                        ),
                    };
                    let (sky_level, block_level) =
                        neighbor_channels(&neighbor_light, neighbor_cell.0, y, neighbor_cell.1);
                    cells[padded_index(ring.0, y, ring.1)] = pack(sky_level, block_level);
                    seed_from_neighbor(
                        chunk,
                        light_table,
                        &mut sky,
                        &mut sky_queue,
                        local.0,
                        y,
                        local.1,
                        sky_level,
                    );
                    seed_from_neighbor(
                        chunk,
                        light_table,
                        &mut block,
                        &mut block_queue,
                        local.0,
                        y,
                        local.1,
                        block_level,
                    );
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
        let ring_corners = [
            (-1, -1),
            (CHUNK_SIZE as i32, -1),
            (-1, CHUNK_SIZE as i32),
            (CHUNK_SIZE as i32, CHUNK_SIZE as i32),
        ];
        for (corner_index, diagonal) in diagonal_neighbors.into_iter().enumerate() {
            let Some(diagonal) = diagonal else {
                continue;
            };
            let diagonal_light = Self::from_chunk(diagonal);
            let (local_x, local_z) = diagonal_coords[corner_index];
            let (ring_x, ring_z) = ring_corners[corner_index];
            for y in 0..CHUNK_HEIGHT {
                let (sky_level, block_level) =
                    neighbor_channels(&diagonal_light, local_x, y, local_z);
                cells[padded_index(ring_x, y, ring_z)] = pack(sky_level, block_level);
            }
        }
        propagate(chunk, light_table, &mut sky, &mut sky_queue);
        propagate(chunk, light_table, &mut block, &mut block_queue);

        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let index = Chunk::index(x, y, z);
                    cells[padded_index(x as i32, y, z as i32)] = pack(sky[index], block[index]);
                }
            }
        }
        Self { cells }
    }

    /// Combined Beta light value at a block, after taking the brighter channel.
    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        self.light(x, y, z)
    }

    pub fn light(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        let (sky, block) = self.center(x, y, z)?;
        Some(sky.max(block))
    }

    pub fn sky(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        self.center(x, y, z).map(|(sky, _)| sky)
    }

    pub fn block(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        self.center(x, y, z).map(|(_, block)| block)
    }

    fn center(&self, x: usize, y: usize, z: usize) -> Option<(u8, u8)> {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return None;
        }
        Some(unpack(self.cells[padded_index(x as i32, y, z as i32)]))
    }

    pub fn get_extended(&self, x: i32, y: i32, z: i32) -> u8 {
        self.light_at(x, y, z, 0)
    }

    /// Packed sky and block light at a chunk-local cell, which may sit in the
    /// one-cell ring around the chunk. Below the world is dark and above it is
    /// open sky. Cells farther out reuse the nearest ring cell.
    pub fn channels_at(&self, x: i32, y: i32, z: i32) -> u8 {
        if y < 0 {
            return 0;
        }
        if y >= CHUNK_HEIGHT as i32 {
            return OPEN_SKY;
        }
        let x = x.clamp(-1, CHUNK_SIZE as i32);
        let z = z.clamp(-1, CHUNK_SIZE as i32);
        self.cells[padded_index(x, y as usize, z)]
    }

    /// Combined light after `Chunk.getBlockLightValue`. `skylight_subtracted`
    /// is the time-of-day penalty from `World.calculateSkylightSubtracted`.
    pub fn light_at(&self, x: i32, y: i32, z: i32, skylight_subtracted: u8) -> u8 {
        let (sky, block) = unpack(self.channels_at(x, y, z));
        combined_light(sky, block, skylight_subtracted)
    }

    /// The chunk's own packed light cells, without the neighbor ring, in
    /// [`Chunk::index`] order. [`LightCache`] keeps this for block ticks.
    pub fn chunk_cells(&self) -> Arc<[u8]> {
        let mut cells = Vec::with_capacity(LIGHT_CELLS);
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE as i32 {
                let row = padded_index(0, y, z);
                cells.extend_from_slice(&self.cells[row..row + CHUNK_SIZE]);
            }
        }
        cells.into()
    }

    /// One hash per render section over every light cell its mesh can sample,
    /// including the ring and the layer above and below. A section whose
    /// fingerprint is unchanged after an edit keeps its mesh.
    pub fn section_fingerprints(&self) -> [u64; SECTIONS_PER_CHUNK] {
        std::array::from_fn(|section| {
            let first = (section * SECTION_HEIGHT).saturating_sub(1);
            let last = ((section + 1) * SECTION_HEIGHT).min(CHUNK_HEIGHT - 1);
            let mut hasher = std::hash::DefaultHasher::new();
            hasher.write(&self.cells[first * PADDED_LAYER..(last + 1) * PADDED_LAYER]);
            hasher.finish()
        })
    }
}

/// The light each loaded chunk had when it was last lit, for simulation that
/// reads light outside a mesh job: grass spread, crop growth, and melting.
///
/// The world keeps no persistent light arrays. Streaming lights every
/// rendered chunk as part of meshing, and this cache keeps the chunk's cells
/// from that pass, so a lookup costs one map probe instead of a relight. A
/// cached value lags an edit until the edited chunk's mesh job finishes, as
/// Beta's queued lighting updates lag its block changes.
#[derive(Resource, Default)]
pub struct LightCache {
    chunks: HashMap<ChunkPosition, Arc<[u8]>>,
}

impl LightCache {
    /// Cells from [`Skylight::chunk_cells`] for `position`.
    pub fn insert(&mut self, position: ChunkPosition, cells: Arc<[u8]>) {
        debug_assert_eq!(cells.len(), LIGHT_CELLS);
        self.chunks.insert(position, cells);
    }

    pub fn remove(&mut self, position: ChunkPosition) {
        self.chunks.remove(&position);
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    pub fn contains(&self, position: ChunkPosition) -> bool {
        self.chunks.contains_key(&position)
    }

    /// Light `position` from the loaded chunks around it now, as a mesh job
    /// would. For tests and tools; the game fills the cache from meshing.
    pub fn relight(&mut self, chunks: &WorldChunks, position: ChunkPosition) {
        let Some(center) = chunks.get(position) else {
            return;
        };
        let neighbor = |dx: i32, dz: i32| {
            chunks
                .get(ChunkPosition {
                    x: position.x + dx,
                    z: position.z + dz,
                })
                .map(|generated| &generated.chunk)
        };
        let light = Skylight::from_chunk_with_neighbors_and_corners(
            &center.chunk,
            neighbor(-1, 0),
            neighbor(1, 0),
            neighbor(0, -1),
            neighbor(0, 1),
            neighbor(-1, -1),
            neighbor(1, -1),
            neighbor(-1, 1),
            neighbor(1, 1),
        );
        self.insert(position, light.chunk_cells());
    }

    /// Sky and block light at a world cell, or `None` when its chunk has not
    /// been lit. Above the world is open sky and below it is dark.
    pub fn channels(&self, x: i32, y: i32, z: i32) -> Option<(u8, u8)> {
        if y < 0 {
            return Some((0, 0));
        }
        if y >= CHUNK_HEIGHT as i32 {
            return Some((MAX_LIGHT, 0));
        }
        let cells = self.chunks.get(&ChunkPosition::from_block(x, z))?;
        let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        Some(unpack(cells[Chunk::index(local_x, y as usize, local_z)]))
    }
}

fn neighbor_channels(light: &Skylight, x: usize, y: usize, z: usize) -> (u8, u8) {
    (
        light.sky(x, y, z).unwrap_or(0),
        light.block(x, y, z).unwrap_or(0),
    )
}

/// The six face neighbors of a cell in a `WIDTH × CHUNK_HEIGHT × WIDTH` volume.
fn neighbors<const WIDTH: usize, const LAYER: usize>(i: usize) -> impl Iterator<Item = usize> {
    let x = i % WIDTH;
    let z = i / WIDTH % WIDTH;
    let y = i / LAYER;
    [
        (x + 1 < WIDTH).then(|| i + 1),
        (x > 0).then(|| i - 1),
        (y + 1 < CHUNK_HEIGHT).then(|| i + LAYER),
        (y > 0).then(|| i - LAYER),
        (z + 1 < WIDTH).then(|| i + WIDTH),
        (z > 0).then(|| i - WIDTH),
    ]
    .into_iter()
    .flatten()
}

fn flood<const WIDTH: usize, const LAYER: usize>(
    light: &mut [u8],
    opacity: &[u8],
    mut queue: VecDeque<u32>,
) {
    while let Some(i) = queue.pop_front() {
        let i = i as usize;
        let source = light[i];
        if source <= 1 {
            continue;
        }
        for n in neighbors::<WIDTH, LAYER>(i) {
            let candidate = source.saturating_sub(opacity[n].max(1));
            if candidate > light[n] {
                light[n] = candidate;
                queue.push_back(n as u32);
            }
        }
    }
}

/// Opacity and emission indexed by the raw block byte stored in chunks.
struct LightTables {
    opacity: [u8; 256],
    emission: [u8; 256],
}

fn light_tables() -> &'static LightTables {
    static TABLES: OnceLock<LightTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let table = crate::block::definition::properties_table();
        let mut tables = LightTables {
            opacity: [0; 256],
            emission: [0; 256],
        };
        for raw in 0..=u8::MAX {
            let block = Id::from(raw);
            tables.opacity[usize::from(raw)] = light_opacity_from_table(table, block);
            tables.emission[usize::from(raw)] = light_emission_from_table(table, block);
        }
        tables
    })
}

/// `Chunk.getBlockLightValue`: sunlight loses `skylight_subtracted` levels as
/// the day ends, then the brighter of that and the block light is kept.
pub fn combined_light(sky: u8, block: u8, skylight_subtracted: u8) -> u8 {
    sky.saturating_sub(skylight_subtracted.min(MAX_LIGHT))
        .max(block.min(MAX_LIGHT))
}

/// Vertex brightness for a light level. This is Beta's
/// `WorldProvider.lightBrightnessTable`, which the block shader evaluates per
/// sample so the time of day never requires rebuilding a mesh.
pub fn beta_brightness(level: u8) -> f32 {
    let level = level.min(MAX_LIGHT) as f32;
    let darkness = 1.0 - level / 15.0;
    let base = 0.05;
    (1.0 - darkness) / (darkness * 3.0 + 1.0) * (1.0 - base) + base
}

/// `World.getBlockLightValue` for a position outside the meshed area (fog,
/// eye brightness, entity shadows). Cheaper than rebuilding a [`Skylight`]
/// snapshot, at the cost of ignoring lateral propagation from a torch just
/// outside this exact cell.
pub fn light_level_at(chunks: &WorldChunks, x: i32, y: i32, z: i32, skylight_subtracted: u8) -> u8 {
    let sky = if open_to_sky(chunks, x, y, z) {
        MAX_LIGHT
    } else {
        0
    };
    let block = chunks.block_at(x, y, z).map_or(0, light_emission);
    combined_light(sky, block, skylight_subtracted)
}

/// Direct sun reaches a position when no opaque block sits in the column
/// above it.
fn open_to_sky(chunks: &WorldChunks, x: i32, y: i32, z: i32) -> bool {
    if y >= CHUNK_HEIGHT as i32 {
        return true;
    }
    for above in (y + 1)..CHUNK_HEIGHT as i32 {
        let Some(block) = chunks.block_at(x, above, z) else {
            return true;
        };
        if light_opacity(block) >= MAX_LIGHT {
            return false;
        }
    }
    true
}

fn seed_sky_edge(
    chunk: &Chunk,
    light_table: &[BlockProperties; 256],
    sky: &mut [u8],
    queue: &mut VecDeque<(usize, usize, usize)>,
    x: usize,
    y: usize,
    z: usize,
) {
    if light_opacity_from_table(light_table, chunk.get(x, y, z).unwrap()) == 0 {
        let index = Chunk::index(x, y, z);
        if sky[index] < MAX_LIGHT {
            sky[index] = MAX_LIGHT;
            queue.push_back((x, y, z));
        }
    }
}

fn seed_from_neighbor(
    chunk: &Chunk,
    light_table: &[BlockProperties; 256],
    sky: &mut [u8],
    queue: &mut VecDeque<(usize, usize, usize)>,
    x: usize,
    y: usize,
    z: usize,
    neighbor_level: u8,
) {
    let level = neighbor_level
        .saturating_sub(light_opacity_from_table(light_table, chunk.get(x, y, z).unwrap()).max(1));
    let index = Chunk::index(x, y, z);
    if level > sky[index] {
        sky[index] = level;
        queue.push_back((x, y, z));
    }
}

fn propagate(
    chunk: &Chunk,
    light_table: &[BlockProperties; 256],
    light: &mut [u8],
    queue: &mut VecDeque<(usize, usize, usize)>,
) {
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
            let attenuation =
                light_opacity_from_table(light_table, chunk.get(nx, ny, nz).unwrap()).max(1);
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
pub fn light_opacity(block: Id) -> u8 {
    crate::block::definition::light_opacity(block)
}

pub fn light_emission(block: Id) -> u8 {
    crate::block::definition::light_emission(block)
}

#[inline]
fn light_opacity_from_table(table: &[BlockProperties; 256], block: Id) -> u8 {
    match block {
        Id::Unknown(_) => 15,
        _ => table[block.as_u8() as usize].light_opacity,
    }
}

#[inline]
fn light_emission_from_table(table: &[BlockProperties; 256], block: Id) -> u8 {
    match block {
        Id::Unknown(_) => 0,
        _ => table[block.as_u8() as usize].light_emission,
    }
}
