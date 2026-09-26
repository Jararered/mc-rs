//! Beta-style tree decoration.
//!
//! Ports the reference `WorldGenTrees`, `WorldGenForest`, `WorldGenTaiga1`,
//! `WorldGenTaiga2`, and `WorldGenBigTree` generators, plus the per-chunk
//! placement loop from `ChunkProviderGenerate.populate`.
//!
//! The reference places trees at `chunk * 16 + rand(16) + 8`, so a chunk's trees
//! spill into its `+x`/`+z` neighbours. To keep generation self-contained and
//! parallel, each chunk replays the placement loop for its 3×3 neighbourhood and
//! rasterises the results into itself. A tree is therefore written by every chunk
//! it overlaps, and canopies stay seamless across chunk borders.
//!
//! Occupancy checks read the tree blocks replayed so far in a shared 3×3
//! neighborhood. Writes outside the target chunk remain in the shared tree view
//! so neighboring trees see the same occupied blocks without slicing canopies at
//! chunk seams. Out-of-chunk origins use the same surface query as
//! [`super::surface::apply_surface`], so a sand column is sand in every neighbour
//! and does not grow a canopy without a trunk.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::block::id::Id;
use crate::block::properties::is_opaque_cube;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

use super::biome::Biome;
use super::biome::Climate;
use super::cactus::place_cacti;
use super::plants::place_dead_bushes;
use super::plants::place_plant_extras;
use super::plants::place_plants;
use super::pumpkin::place_pumpkins;
use super::reeds::place_reeds;
use super::terrain::TerrainGenerator;
use crate::random::JavaRandom;

/// The reference's `WorldGenBigTree.field_882_a`: for each axis, the two other
/// axes in a fixed order, used to walk a line along its longest axis.
const AXIS_ORDER: [usize; 6] = [2, 0, 0, 1, 2, 1];
/// The widest big-tree leaf cluster can reach eight blocks from its trunk.
const MAX_TREE_RADIUS: i32 = 8;

/// Which generator a biome rolls for a placement.
#[derive(Clone, Copy)]
enum TreeKind {
    Oak,
    Birch,
    Spruce1,
    Spruce2,
    Big,
}

/// The world view the tree generators read and write through.
///
/// Coordinates are chunk-local and may fall outside `0..CHUNK_SIZE`. Reads
/// outside the target consult the neighboring base chunks and shared tree
/// overrides; the block directly below the current tree's origin reports the
/// ground block found by decoration.
struct TreeWorld<'a> {
    target: ChunkPos,
    chunk: &'a mut Chunk,
    origin: [i32; 3],
    ground: Id,
    remote_chunks: &'a HashMap<ChunkPos, Chunk>,
    overrides: &'a mut HashMap<(i32, i32, i32), Id>,
}

impl TreeWorld<'_> {
    fn in_bounds(x: i32, y: i32, z: i32) -> bool {
        (0..CHUNK_SIZE as i32).contains(&x)
            && (0..CHUNK_HEIGHT as i32).contains(&y)
            && (0..CHUNK_SIZE as i32).contains(&z)
    }

    fn get(&self, x: i32, y: i32, z: i32) -> Id {
        let current = if let Some(block) = self.overrides.get(&(x, y, z)) {
            *block
        } else if Self::in_bounds(x, y, z) {
            self.chunk.get(x as usize, y as usize, z as usize).unwrap()
        } else if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            Id::Air
        } else {
            let world_x = self.target.x * CHUNK_SIZE as i32 + x;
            let world_z = self.target.z * CHUNK_SIZE as i32 + z;
            let position = ChunkPos::from_block(world_x, world_z);
            self.remote_chunks
                .get(&position)
                .and_then(|chunk| {
                    chunk.get(
                        world_x.rem_euclid(CHUNK_SIZE as i32) as usize,
                        y as usize,
                        world_z.rem_euclid(CHUNK_SIZE as i32) as usize,
                    )
                })
                .unwrap_or(Id::Air)
        };
        if x == self.origin[0] && z == self.origin[2] && y == self.origin[1] - 1 {
            if is_log(current) || is_leaf(current) {
                current
            } else {
                self.ground
            }
        } else {
            current
        }
    }

    fn set(&mut self, x: i32, y: i32, z: i32, block: Id) {
        let current = self.get(x, y, z);
        if is_log(block) {
            if is_log(current) {
                return;
            }
            let below = (0..y.max(0)).rev().find_map(|by| {
                let found = self.get(x, by, z);
                if matches!(
                    found,
                    Id::Wood
                        | Id::BirchWood
                        | Id::SpruceWood
                        | Id::Leaves
                        | Id::BirchLeaves
                        | Id::SpruceLeaves
                        | Id::Air
                ) {
                    None
                } else {
                    Some(found)
                }
            });
            if below == Some(Id::Sand) {
                return;
            }
        } else if is_leaf(block) && is_opaque_cube(current) {
            return;
        }
        if Self::in_bounds(x, y, z) {
            self.chunk.set(x as usize, y as usize, z as usize, block);
        } else if (0..CHUNK_HEIGHT as i32).contains(&y) {
            self.overrides.insert((x, y, z), block);
        }
    }

    fn is_opaque(&self, x: i32, y: i32, z: i32) -> bool {
        is_opaque_cube(self.get(x, y, z))
    }
}

fn is_log(block: Id) -> bool {
    matches!(block, Id::Wood | Id::BirchWood | Id::SpruceWood)
}

fn is_leaf(block: Id) -> bool {
    matches!(block, Id::Leaves | Id::BirchLeaves | Id::SpruceLeaves)
}

fn is_tree_space(block: Id) -> bool {
    matches!(block, Id::Air) || is_leaf(block)
}

/// Replay the reference placement loop for every chunk overlapping `position`
/// and write the resulting trees into `chunk`.
pub(super) fn decorate(
    chunk: &mut Chunk,
    position: ChunkPos,
    terrain: &TerrainGenerator,
    population_rng: &HashMap<ChunkPos, JavaRandom>,
    climate_at: impl Fn(f64, f64) -> Climate,
    remote_chunk: impl Fn(ChunkPos) -> Chunk,
) {
    // Ground heights and surface blocks are captured before any tree is placed
    // so placement sees the terrain rather than earlier trees.
    let mut heights = [[0u8; CHUNK_SIZE]; CHUNK_SIZE];
    let mut grounds = [[Id::Air; CHUNK_SIZE]; CHUNK_SIZE];
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            let height = top_non_air(chunk, x, z);
            heights[x][z] = height as u8;
            grounds[x][z] = chunk.get(x, height.saturating_sub(1), z).unwrap_or(Id::Air);
        }
    }

    let mut remote_ground = HashMap::new();
    let mut remote_chunks = HashMap::new();
    let mut tree_overrides = HashMap::new();
    let mut cactus_positions = HashSet::new();
    let mut reed_positions = HashSet::new();
    let mut pumpkin_positions = HashSet::new();
    for source_z in -1..=1 {
        for source_x in -1..=1 {
            let source = ChunkPos {
                x: position.x + source_x,
                z: position.z + source_z,
            };
            populate(
                chunk,
                position,
                source,
                &heights,
                &grounds,
                &mut remote_ground,
                &mut remote_chunks,
                &mut tree_overrides,
                &mut cactus_positions,
                &mut reed_positions,
                &mut pumpkin_positions,
                terrain,
                population_rng,
                &climate_at,
                &remote_chunk,
            );
        }
    }
}

/// Run the reference `populate` tree loop for `source`, rasterising into the
/// chunk at `target`.
#[allow(clippy::too_many_arguments)]
fn populate(
    chunk: &mut Chunk,
    target: ChunkPos,
    source: ChunkPos,
    heights: &[[u8; CHUNK_SIZE]; CHUNK_SIZE],
    grounds: &[[Id; CHUNK_SIZE]; CHUNK_SIZE],
    remote_ground: &mut HashMap<(i32, i32), (i32, Id)>,
    remote_chunks: &mut HashMap<ChunkPos, Chunk>,
    tree_overrides: &mut HashMap<(i32, i32, i32), Id>,
    cactus_positions: &mut HashSet<(i32, i32, i32)>,
    reed_positions: &mut HashSet<(i32, i32, i32)>,
    pumpkin_positions: &mut HashSet<(i32, i32, i32)>,
    terrain: &TerrainGenerator,
    population_rng: &HashMap<ChunkPos, JavaRandom>,
    climate_at: &impl Fn(f64, f64) -> Climate,
    remote_chunk: &impl Fn(ChunkPos) -> Chunk,
) {
    let mut rand = population_rng[&source].clone();

    let biome = climate_at(
        (source.x * CHUNK_SIZE as i32 + CHUNK_SIZE as i32) as f64,
        (source.z * CHUNK_SIZE as i32 + CHUNK_SIZE as i32) as f64,
    )
    .biome;

    let density =
        terrain
            .mob_spawner
            .sample_2d((source.x * 8) as f64, (source.z * 8) as f64, 1.0, 1.0);
    let base_count = ((density / 8.0 + rand.next_double() * 4.0 + 4.0) / 3.0) as i32;
    let mut count = 0;
    if rand.next_int(10) == 0 {
        count += 1;
    }
    match biome {
        Biome::Forest | Biome::Rainforest | Biome::Taiga => count += base_count + 5,
        Biome::SeasonalForest => count += base_count + 2,
        Biome::Desert | Biome::Tundra | Biome::Plains => count -= 20,
        _ => {}
    }

    let offset_x = (source.x - target.x) * CHUNK_SIZE as i32;
    let offset_z = (source.z - target.z) * CHUNK_SIZE as i32;

    for remote_z in -1..=1 {
        for remote_x in -1..=1 {
            let position = ChunkPos {
                x: target.x + remote_x,
                z: target.z + remote_z,
            };
            if position != target {
                remote_chunks
                    .entry(position)
                    .or_insert_with(|| remote_chunk(position));
            }
        }
    }
    let tree_chunks = remote_chunks.clone();

    {
        let mut world = TreeWorld {
            target,
            chunk,
            origin: [0, 0, 0],
            ground: Id::Air,
            remote_chunks: &tree_chunks,
            overrides: tree_overrides,
        };
        for _ in 0..count {
            let local_x = rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
            let local_z = rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
            let kind = select_tree(biome, &mut rand);
            let world_x = source.x * CHUNK_SIZE as i32 + local_x;
            let world_z = source.z * CHUNK_SIZE as i32 + local_z;
            let origin_x = local_x + offset_x;
            let origin_z = local_z + offset_z;
            // A tree's private RNG cannot affect later placements. Keep its seed
            // draw, but avoid looking up remote ground if no branch can reach us.
            let tree_seed = rand.next_long() as u64;
            if origin_x + MAX_TREE_RADIUS < 0
                || origin_x - MAX_TREE_RADIUS >= CHUNK_SIZE as i32
                || origin_z + MAX_TREE_RADIUS < 0
                || origin_z - MAX_TREE_RADIUS >= CHUNK_SIZE as i32
            {
                continue;
            }
            let (height, ground) = ground_at(
                target,
                heights,
                grounds,
                remote_ground,
                remote_chunks,
                remote_chunk,
                world_x,
                world_z,
            );

            let origin = [origin_x, height, origin_z];
            world.origin = origin;
            world.ground = ground;
            // A private RNG per tree so a failed ground check in one chunk cannot
            // desynchronise later trees when the neighbour does plant it.
            let mut tree_rand = JavaRandom::new(tree_seed);
            generate(kind, &mut world, &mut tree_rand, origin);
        }
    }

    // Flowers and tall grass follow trees on this same random sequence.
    // Drawing them earlier would move every tree.
    place_plants(
        chunk,
        target,
        source,
        &mut rand,
        biome,
        |world_x, world_z| {
            ground_at(
                target,
                heights,
                grounds,
                remote_ground,
                remote_chunks,
                remote_chunk,
                world_x,
                world_z,
            )
            .0
        },
    );
    place_dead_bushes(
        chunk,
        target,
        source,
        &mut rand,
        biome,
        remote_chunks,
        remote_chunk,
    );
    place_plant_extras(chunk, target, source, &mut rand);
    place_reeds(
        chunk,
        target,
        source,
        &mut rand,
        reed_positions,
        remote_chunks,
        remote_chunk,
    );
    place_pumpkins(
        chunk,
        target,
        source,
        &mut rand,
        pumpkin_positions,
        remote_chunks,
        remote_chunk,
    );
    place_cacti(
        chunk,
        target,
        source,
        &mut rand,
        biome,
        cactus_positions,
        remote_chunks,
        remote_chunk,
    );
}

fn select_tree(biome: Biome, rand: &mut JavaRandom) -> TreeKind {
    match biome {
        Biome::Rainforest => {
            if rand.next_int(3) == 0 {
                TreeKind::Big
            } else {
                TreeKind::Oak
            }
        }
        Biome::Forest => {
            if rand.next_int(5) == 0 {
                TreeKind::Birch
            } else if rand.next_int(3) == 0 {
                TreeKind::Big
            } else {
                TreeKind::Oak
            }
        }
        Biome::Taiga => {
            if rand.next_int(3) == 0 {
                TreeKind::Spruce1
            } else {
                TreeKind::Spruce2
            }
        }
        _ => {
            if rand.next_int(10) == 0 {
                TreeKind::Big
            } else {
                TreeKind::Oak
            }
        }
    }
}

fn generate(kind: TreeKind, world: &mut TreeWorld, rand: &mut JavaRandom, origin: [i32; 3]) {
    let [x, y, z] = origin;
    match kind {
        TreeKind::Oak => {
            generate_standard(world, rand, x, y, z, Id::Wood, Id::Leaves, 4);
        }
        TreeKind::Birch => {
            generate_standard(world, rand, x, y, z, Id::BirchWood, Id::BirchLeaves, 5);
        }
        TreeKind::Spruce1 => {
            generate_taiga1(world, rand, x, y, z);
        }
        TreeKind::Spruce2 => {
            generate_taiga2(world, rand, x, y, z);
        }
        TreeKind::Big => {
            BigTree::new().generate(world, rand, x, y, z);
        }
    }
}

/// Ground height and surface block at a world column.
///
/// Columns inside the target chunk use the pre-decoration heightmap; columns
/// outside use the same undecorated generation as the neighbour chunk.
fn ground_at(
    target: ChunkPos,
    heights: &[[u8; CHUNK_SIZE]; CHUNK_SIZE],
    grounds: &[[Id; CHUNK_SIZE]; CHUNK_SIZE],
    remote_ground: &mut HashMap<(i32, i32), (i32, Id)>,
    remote_chunks: &mut HashMap<ChunkPos, Chunk>,
    remote_chunk: &impl Fn(ChunkPos) -> Chunk,
    world_x: i32,
    world_z: i32,
) -> (i32, Id) {
    let local_x = world_x - target.x * CHUNK_SIZE as i32;
    let local_z = world_z - target.z * CHUNK_SIZE as i32;
    if (0..CHUNK_SIZE as i32).contains(&local_x) && (0..CHUNK_SIZE as i32).contains(&local_z) {
        (
            heights[local_x as usize][local_z as usize] as i32,
            grounds[local_x as usize][local_z as usize],
        )
    } else if let Some(&column) = remote_ground.get(&(world_x, world_z)) {
        column
    } else {
        let pos = ChunkPos {
            x: world_x.div_euclid(CHUNK_SIZE as i32),
            z: world_z.div_euclid(CHUNK_SIZE as i32),
        };
        let chunk = remote_chunks
            .entry(pos)
            .or_insert_with(|| remote_chunk(pos));
        let x = world_x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let z = world_z.rem_euclid(CHUNK_SIZE as i32) as usize;
        let height = top_non_air(chunk, x, z);
        let column = (
            height as i32,
            chunk.get(x, height.saturating_sub(1), z).unwrap_or(Id::Air),
        );
        remote_ground.insert((world_x, world_z), column);
        column
    }
}

fn top_non_air(chunk: &Chunk, x: usize, z: usize) -> usize {
    (0..CHUNK_HEIGHT)
        .rev()
        .find(|&y| chunk.get(x, y, z).unwrap() != Id::Air)
        .map_or(0, |y| y + 1)
}

fn standard_space_is_clear(world: &TreeWorld, x: i32, y: i32, z: i32, height: i32) -> bool {
    for level in y..=y + 1 + height {
        let radius = if level == y {
            0
        } else if level >= y + height - 1 {
            2
        } else {
            1
        };
        for lx in x - radius..=x + radius {
            for lz in z - radius..=z + radius {
                if !is_tree_space(world.get(lx, level, lz)) {
                    return false;
                }
            }
        }
    }
    true
}

fn taiga_space_is_clear(
    world: &TreeWorld,
    x: i32,
    y: i32,
    z: i32,
    height: i32,
    trunk: i32,
    max_radius: i32,
) -> bool {
    for level in y..=y + 1 + height {
        let radius = if level - y < trunk { 0 } else { max_radius };
        for lx in x - radius..=x + radius {
            for lz in z - radius..=z + radius {
                if !is_tree_space(world.get(lx, level, lz)) {
                    return false;
                }
            }
        }
    }
    true
}

/// `WorldGenTrees` and `WorldGenForest`, which differ only in trunk height and
/// the wood/leaf species.
#[allow(clippy::too_many_arguments)]
fn generate_standard(
    world: &mut TreeWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
    wood: Id,
    leaves: Id,
    base_height: i32,
) -> bool {
    let height = rand.next_int(3) as i32 + base_height;
    if y >= 1 && y + height < CHUNK_HEIGHT as i32 && standard_space_is_clear(world, x, y, z, height)
    {
        let below = world.get(x, y - 1, z);
        if (below == Id::Grass || below == Id::Dirt) && y < CHUNK_HEIGHT as i32 - height - 1 {
            world.set(x, y - 1, z, Id::Dirt);
            for level in y - 3 + height..=y + height {
                let offset = level - (y + height);
                let radius = 1 - offset / 2;
                for lx in x - radius..=x + radius {
                    let dx = lx - x;
                    for lz in z - radius..=z + radius {
                        let dz = lz - z;
                        if (dx.abs() != radius
                            || dz.abs() != radius
                            || rand.next_int(2) != 0 && offset != 0)
                            && !world.is_opaque(lx, level, lz)
                        {
                            world.set(lx, level, lz, leaves);
                        }
                    }
                }
            }
            for level in 0..height {
                let block = world.get(x, y + level, z);
                if block == Id::Air || is_leaf(block) {
                    world.set(x, y + level, z, wood);
                }
            }
            true
        } else {
            false
        }
    } else {
        false
    }
}

/// `WorldGenTaiga1`: a narrow spruce with a tapering canopy.
fn generate_taiga1(world: &mut TreeWorld, rand: &mut JavaRandom, x: i32, y: i32, z: i32) -> bool {
    let height = rand.next_int(5) as i32 + 7;
    let trunk = height - rand.next_int(2) as i32 - 3;
    let span = height - trunk;
    let max_radius = 1 + rand.next_int((span + 1) as u32) as i32;
    if y >= 1
        && y + height < CHUNK_HEIGHT as i32
        && taiga_space_is_clear(world, x, y, z, height, trunk, max_radius)
    {
        let below = world.get(x, y - 1, z);
        if (below == Id::Grass || below == Id::Dirt) && y < CHUNK_HEIGHT as i32 - height - 1 {
            world.set(x, y - 1, z, Id::Dirt);
            let mut radius = 0;
            let mut level = y + height;
            while level >= y + trunk {
                for lx in x - radius..=x + radius {
                    let dx = lx - x;
                    for lz in z - radius..=z + radius {
                        let dz = lz - z;
                        if (dx.abs() != radius || dz.abs() != radius || radius <= 0)
                            && !world.is_opaque(lx, level, lz)
                        {
                            world.set(lx, level, lz, Id::SpruceLeaves);
                        }
                    }
                }
                if radius >= 1 && level == y + trunk + 1 {
                    radius -= 1;
                } else if radius < max_radius {
                    radius += 1;
                }
                level -= 1;
            }
            for level in 0..height - 1 {
                let block = world.get(x, y + level, z);
                if block == Id::Air || is_leaf(block) {
                    world.set(x, y + level, z, Id::SpruceWood);
                }
            }
            true
        } else {
            false
        }
    } else {
        false
    }
}

/// `WorldGenTaiga2`: a wider spruce with a rounded canopy.
fn generate_taiga2(world: &mut TreeWorld, rand: &mut JavaRandom, x: i32, y: i32, z: i32) -> bool {
    let height = rand.next_int(4) as i32 + 6;
    let trunk = 1 + rand.next_int(2) as i32;
    let span = height - trunk;
    let max_radius = 2 + rand.next_int(2) as i32;
    if y >= 1
        && y + height < CHUNK_HEIGHT as i32
        && taiga_space_is_clear(world, x, y, z, height, trunk, max_radius)
    {
        let below = world.get(x, y - 1, z);
        if (below == Id::Grass || below == Id::Dirt) && y < CHUNK_HEIGHT as i32 - height - 1 {
            world.set(x, y - 1, z, Id::Dirt);
            let mut radius = rand.next_int(2) as i32;
            let mut threshold = 1;
            let mut next_radius = 0;
            for step in 0..=span {
                let level = y + height - step;
                for lx in x - radius..=x + radius {
                    let dx = lx - x;
                    for lz in z - radius..=z + radius {
                        let dz = lz - z;
                        if (dx.abs() != radius || dz.abs() != radius || radius <= 0)
                            && !world.is_opaque(lx, level, lz)
                        {
                            world.set(lx, level, lz, Id::SpruceLeaves);
                        }
                    }
                }
                if radius >= threshold {
                    radius = next_radius;
                    next_radius = 1;
                    threshold += 1;
                    if threshold > max_radius {
                        threshold = max_radius;
                    }
                } else {
                    radius += 1;
                }
            }
            let trunk_height = rand.next_int(3) as i32;
            for level in 0..height - trunk_height {
                let block = world.get(x, y + level, z);
                if block == Id::Air || is_leaf(block) {
                    world.set(x, y + level, z, Id::SpruceWood);
                }
            }
            true
        } else {
            false
        }
    } else {
        false
    }
}

/// The reference scatters branches over `2 * 3.14159` radians, using a
/// truncated pi. Keep the literal so branch placement matches.
#[allow(clippy::approx_constant)]
const BRANCH_ANGLE_SCALE: f64 = 2.0 * 3.14159;

/// `WorldGenBigTree`: a large oak with randomised branches and leaf clusters.
struct BigTree {
    random: JavaRandom,
    base: [i32; 3],
    /// `field_878_e`: total tree height.
    height: i32,
    /// `height`: trunk height, derived from `height`.
    trunk_height: i32,
    clusters: Vec<[i32; 4]>,
}

impl BigTree {
    const TRUNK_SCALE: f64 = 0.618;
    const BRANCH_DROP: f64 = 0.381;
    const BRANCH_SCALE: f64 = 1.0;
    const CLUSTER_SCALE: f64 = 1.0;
    const MAX_HEIGHT: i32 = 12;
    const CLUSTER_HEIGHT: i32 = 4;

    fn new() -> Self {
        Self {
            random: JavaRandom::new(0),
            base: [0, 0, 0],
            height: 0,
            trunk_height: 0,
            clusters: Vec::new(),
        }
    }

    fn generate(
        &mut self,
        world: &mut TreeWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
    ) -> bool {
        self.random = JavaRandom::new(rand.next_long() as u64);
        self.base = [x, y, z];
        if self.height == 0 {
            self.height = 5 + self.random.next_int(Self::MAX_HEIGHT as u32) as i32;
        }
        if !self.can_grow(world) {
            return false;
        }
        self.make_clusters(world);
        self.place_clusters(world);
        self.place_trunk(world);
        self.place_branches(world);
        true
    }

    /// `func_519_e`: check the ground and clear trunk space, shrinking the tree
    /// to fit under an obstruction.
    fn can_grow(&mut self, world: &TreeWorld) -> bool {
        let base = self.base;
        let top = [base[0], base[1] + self.height - 1, base[2]];
        let below = world.get(base[0], base[1] - 1, base[2]);
        if below != Id::Grass && below != Id::Dirt {
            return false;
        }
        let distance = self.line_clearance(world, base, top);
        if distance == -1 {
            true
        } else if distance < 6 {
            false
        } else {
            self.height = distance;
            true
        }
    }

    fn line_clearance(&self, world: &TreeWorld, from: [i32; 3], to: [i32; 3]) -> i32 {
        let mut delta = [0i32; 3];
        let mut axis = 0usize;
        for i in 0..3 {
            delta[i] = to[i] - from[i];
            if delta[i].abs() > delta[axis].abs() {
                axis = i;
            }
        }
        if delta[axis] == 0 {
            return -1;
        }
        let a = AXIS_ORDER[axis];
        let b = AXIS_ORDER[axis + 3];
        let step = if delta[axis] > 0 { 1 } else { -1 };
        let slope_a = delta[a] as f64 / delta[axis] as f64;
        let slope_b = delta[b] as f64 / delta[axis] as f64;
        let mut pos = [0i32; 3];
        let mut offset = 0i32;
        let end = delta[axis] + step;
        while offset != end {
            pos[axis] = (from[axis] as f64 + offset as f64 + 0.5).floor() as i32;
            pos[a] = (from[a] as f64 + offset as f64 * slope_a + 0.5).floor() as i32;
            pos[b] = (from[b] as f64 + offset as f64 * slope_b + 0.5).floor() as i32;
            if !is_tree_space(world.get(pos[0], pos[1], pos[2])) {
                return offset.abs();
            }
            offset += step;
        }
        -1
    }

    /// `func_521_a`: choose branch tips and the leaf clusters they carry.
    fn make_clusters(&mut self, world: &TreeWorld) {
        self.trunk_height = (self.height as f64 * Self::TRUNK_SCALE) as i32;
        if self.trunk_height >= self.height {
            self.trunk_height = self.height - 1;
        }
        let mut cluster_count =
            (1.382 + (Self::CLUSTER_SCALE * self.height as f64 / 13.0).powi(2)) as i32;
        if cluster_count < 1 {
            cluster_count = 1;
        }

        let mut clusters = Vec::new();
        let mut level = self.base[1] + self.height - Self::CLUSTER_HEIGHT;
        let top = self.base[1] + self.trunk_height;
        let mut remaining = self.height - Self::CLUSTER_HEIGHT;
        clusters.push([self.base[0], level, self.base[2], top]);
        level -= 1;

        while remaining >= 0 {
            let radius = self.cluster_radius(remaining);
            if radius < 0.0 {
                level -= 1;
                remaining -= 1;
                continue;
            }
            for _ in 0..cluster_count {
                let distance =
                    Self::BRANCH_SCALE * radius as f64 * (self.random.next_float() as f64 + 0.328);
                let angle = self.random.next_float() as f64 * BRANCH_ANGLE_SCALE;
                let cx = (distance * angle.sin() + self.base[0] as f64 + 0.5).floor() as i32;
                let cz = (distance * angle.cos() + self.base[2] as f64 + 0.5).floor() as i32;
                let tip = [cx, level, cz];
                let tip_top = [cx, level + Self::CLUSTER_HEIGHT, cz];
                if self.line_clearance(world, tip, tip_top) != -1 {
                    continue;
                }
                let mut branch_base = [self.base[0], self.base[1], self.base[2]];
                let horizontal = (((self.base[0] - cx).abs() as f64).powi(2)
                    + ((self.base[2] - cz).abs() as f64).powi(2))
                .sqrt();
                let drop = horizontal * Self::BRANCH_DROP;
                branch_base[1] = if tip[1] as f64 - drop > top as f64 {
                    top
                } else {
                    (tip[1] as f64 - drop) as i32
                };
                if self.line_clearance(world, branch_base, tip) == -1 {
                    clusters.push([cx, level, cz, branch_base[1]]);
                }
            }
            level -= 1;
            remaining -= 1;
        }
        self.clusters = clusters;
    }

    /// `func_528_a`: canopy radius at a height above the base.
    fn cluster_radius(&self, level: i32) -> f32 {
        if (level as f64) < self.height as f64 * 0.3 {
            return -1.618;
        }
        let half = self.height as f32 / 2.0;
        let offset = self.height as f32 / 2.0 - level as f32;
        let mut radius = if offset == 0.0 {
            half
        } else if offset.abs() >= half {
            0.0
        } else {
            (half.abs().powi(2) - offset.abs().powi(2)).sqrt()
        };
        radius *= 0.5;
        radius
    }

    /// `func_526_b`: leaf blob radius within a cluster.
    fn blob_radius(&self, offset: i32) -> f32 {
        if (0..Self::CLUSTER_HEIGHT).contains(&offset) {
            if offset != 0 && offset != Self::CLUSTER_HEIGHT - 1 {
                3.0
            } else {
                2.0
            }
        } else {
            -1.0
        }
    }

    /// `func_518_b`: place a leaf blob at every cluster.
    fn place_clusters(&self, world: &mut TreeWorld) {
        for cluster in &self.clusters {
            for level in cluster[1]..cluster[1] + Self::CLUSTER_HEIGHT {
                let radius = self.blob_radius(level - cluster[1]);
                self.place_leaf_sphere(world, cluster[0], level, cluster[2], radius);
            }
        }
    }

    /// `func_523_a`: place leaves in a disc perpendicular to `axis`.
    fn place_leaf_sphere(&self, world: &mut TreeWorld, x: i32, y: i32, z: i32, radius: f32) {
        let extent = (radius as f64 + 0.618) as i32;
        let a = AXIS_ORDER[1];
        let b = AXIS_ORDER[4];
        let center = [x, y, z];
        let mut pos = center;
        for offset_a in -extent..=extent {
            pos[a] = center[a] + offset_a;
            for offset_b in -extent..=extent {
                let distance = (((offset_a.abs() as f64) + 0.5).powi(2)
                    + ((offset_b.abs() as f64) + 0.5).powi(2))
                .sqrt();
                if distance > radius as f64 {
                    continue;
                }
                pos[b] = center[b] + offset_b;
                let existing = world.get(pos[0], pos[1], pos[2]);
                if existing != Id::Air && existing != Id::Leaves {
                    continue;
                }
                world.set(pos[0], pos[1], pos[2], Id::Leaves);
            }
        }
    }

    /// `func_529_c`: draw the trunk.
    fn place_trunk(&self, world: &mut TreeWorld) {
        let base = self.base;
        let top = [base[0], base[1] + self.trunk_height, base[2]];
        self.draw_line(world, base, top);
    }

    /// `func_525_d`: draw a branch from the trunk to each cluster.
    fn place_branches(&self, world: &mut TreeWorld) {
        let mut from = [self.base[0], self.base[1], self.base[2]];
        for cluster in &self.clusters {
            let to = [cluster[0], cluster[1], cluster[2]];
            from[1] = cluster[3];
            let length = from[1] - self.base[1];
            if length as f64 >= self.height as f64 * 0.2 {
                self.draw_line(world, from, to);
            }
        }
    }

    /// `func_522_a`: draw a line of wood between two points.
    fn draw_line(&self, world: &mut TreeWorld, from: [i32; 3], to: [i32; 3]) {
        let mut delta = [0i32; 3];
        let mut axis = 0usize;
        for i in 0..3 {
            delta[i] = to[i] - from[i];
            if delta[i].abs() > delta[axis].abs() {
                axis = i;
            }
        }
        if delta[axis] == 0 {
            return;
        }
        let a = AXIS_ORDER[axis];
        let b = AXIS_ORDER[axis + 3];
        let step = if delta[axis] > 0 { 1 } else { -1 };
        let slope_a = delta[a] as f64 / delta[axis] as f64;
        let slope_b = delta[b] as f64 / delta[axis] as f64;
        let mut pos = [0i32; 3];
        let mut offset = 0i32;
        let end = delta[axis] + step;
        while offset != end {
            pos[axis] = (from[axis] as f64 + offset as f64 + 0.5).floor() as i32;
            pos[a] = (from[a] as f64 + offset as f64 * slope_a + 0.5).floor() as i32;
            pos[b] = (from[b] as f64 + offset as f64 * slope_b + 0.5).floor() as i32;
            world.set(pos[0], pos[1], pos[2], Id::Wood);
            offset += step;
        }
    }
}
