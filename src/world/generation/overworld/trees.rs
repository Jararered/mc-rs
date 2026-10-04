//! Beta `WorldGenTrees`, `WorldGenForest`, `WorldGenTaiga1`, `WorldGenTaiga2`,
//! and `WorldGenBigTree`, run on the population world with the population
//! random, so each tree consumes exactly the draws Beta's does.

use crate::block::blocks::Block;
use crate::block::properties::is_opaque_cube;
use crate::random::JavaRandom;
use crate::world::chunk::CHUNK_HEIGHT;

use super::world::PopulationWorld;
use super::world::is_air_or_leaves;
use crate::world::biome::Biome;

const HEIGHT: i32 = CHUNK_HEIGHT as i32;

/// The reference's `WorldGenBigTree.field_882_a`: for each axis, the two other
/// axes in a fixed order, used to walk a line along its longest axis.
const AXIS_ORDER: [usize; 6] = [2, 0, 0, 1, 2, 1];

/// Which generator a biome rolls for a placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TreeKind {
    Oak,
    Birch,
    Spruce1,
    Spruce2,
    Big,
}

/// `BiomeGenBase.getRandomWorldGenForTrees` and its overrides.
pub(super) fn select_tree(biome: Biome, rand: &mut JavaRandom) -> TreeKind {
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

pub(super) fn generate_tree(
    kind: TreeKind,
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
) -> bool {
    match kind {
        TreeKind::Oak => generate_standard(world, rand, x, y, z, Block::Wood, Block::Leaves, 4),
        TreeKind::Birch => generate_standard(
            world,
            rand,
            x,
            y,
            z,
            Block::BirchWood,
            Block::BirchLeaves,
            5,
        ),
        TreeKind::Spruce1 => generate_taiga1(world, rand, x, y, z),
        TreeKind::Spruce2 => generate_taiga2(world, rand, x, y, z),
        // Population calls `func_517_a(1.0, 1.0, 1.0)` on every generator,
        // which gives big trees five-layer leaf clusters.
        TreeKind::Big => BigTree::new(1.0, 1.0, 1.0).generate(world, rand, x, y, z),
    }
}

/// The space checks shared by every small tree: `radius(level)` blocks
/// around the trunk must hold only air or leaves, inside the world.
fn space_is_clear(
    world: &PopulationWorld,
    x: i32,
    y: i32,
    z: i32,
    height: i32,
    radius: impl Fn(i32) -> i32,
) -> bool {
    for level in y..=y + 1 + height {
        let radius = radius(level);
        for lx in x - radius..=x + radius {
            for lz in z - radius..=z + radius {
                if !(0..HEIGHT).contains(&level) || !is_air_or_leaves(world.get(lx, level, lz)) {
                    return false;
                }
            }
        }
    }
    true
}

fn grows_on(block: Block) -> bool {
    matches!(block, Block::Grass | Block::Dirt)
}

/// `WorldGenTrees` and `WorldGenForest`, which differ only in trunk height and
/// the wood/leaf species.
#[allow(clippy::too_many_arguments)]
fn generate_standard(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
    wood: Block,
    leaves: Block,
    base_height: i32,
) -> bool {
    let height = rand.next_int(3) as i32 + base_height;
    if y < 1 || y + height + 1 > HEIGHT {
        return false;
    }
    let clear = space_is_clear(world, x, y, z, height, |level| {
        if level >= y + 1 + height - 2 {
            2
        } else if level == y {
            0
        } else {
            1
        }
    });
    if !clear || !grows_on(world.get(x, y - 1, z)) || y >= HEIGHT - height - 1 {
        return false;
    }
    world.set(x, y - 1, z, Block::Dirt);
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
                    && !is_opaque_cube(world.get(lx, level, lz))
                {
                    world.set(lx, level, lz, leaves);
                }
            }
        }
    }
    for level in 0..height {
        if is_air_or_leaves(world.get(x, y + level, z)) {
            world.set(x, y + level, z, wood);
        }
    }
    true
}

/// `WorldGenTaiga1`: a narrow spruce with a tapering canopy.
fn generate_taiga1(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
) -> bool {
    let height = rand.next_int(5) as i32 + 7;
    let trunk = height - rand.next_int(2) as i32 - 3;
    let span = height - trunk;
    let max_radius = 1 + rand.next_int((span + 1) as u32) as i32;
    if y < 1 || y + height + 1 > HEIGHT {
        return false;
    }
    let clear = space_is_clear(world, x, y, z, height, |level| {
        if level - y < trunk { 0 } else { max_radius }
    });
    if !clear || !grows_on(world.get(x, y - 1, z)) || y >= HEIGHT - height - 1 {
        return false;
    }
    world.set(x, y - 1, z, Block::Dirt);
    let mut radius = 0;
    let mut level = y + height;
    while level >= y + trunk {
        for lx in x - radius..=x + radius {
            let dx = lx - x;
            for lz in z - radius..=z + radius {
                let dz = lz - z;
                if (dx.abs() != radius || dz.abs() != radius || radius <= 0)
                    && !is_opaque_cube(world.get(lx, level, lz))
                {
                    world.set(lx, level, lz, Block::SpruceLeaves);
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
        if is_air_or_leaves(world.get(x, y + level, z)) {
            world.set(x, y + level, z, Block::SpruceWood);
        }
    }
    true
}

/// `WorldGenTaiga2`: a wider spruce with a rounded canopy.
fn generate_taiga2(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
) -> bool {
    let height = rand.next_int(4) as i32 + 6;
    let trunk = 1 + rand.next_int(2) as i32;
    let span = height - trunk;
    let max_radius = 2 + rand.next_int(2) as i32;
    if y < 1 || y + height + 1 > HEIGHT {
        return false;
    }
    let clear = space_is_clear(world, x, y, z, height, |level| {
        if level - y < trunk { 0 } else { max_radius }
    });
    if !clear || !grows_on(world.get(x, y - 1, z)) || y >= HEIGHT - height - 1 {
        return false;
    }
    world.set(x, y - 1, z, Block::Dirt);
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
                    && !is_opaque_cube(world.get(lx, level, lz))
                {
                    world.set(lx, level, lz, Block::SpruceLeaves);
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
    let trunk_gap = rand.next_int(3) as i32;
    for level in 0..height - trunk_gap {
        if is_air_or_leaves(world.get(x, y + level, z)) {
            world.set(x, y + level, z, Block::SpruceWood);
        }
    }
    true
}

/// The reference scatters branches over `2 * 3.14159` radians, using a
/// truncated pi. Keep the literal so branch placement matches.
#[allow(clippy::approx_constant)]
const BRANCH_ANGLE_SCALE: f64 = 3.14159;

/// `WorldGenBigTree`: a large oak with randomised branches and leaf clusters.
struct BigTree {
    random: JavaRandom,
    base: [i32; 3],
    /// `field_878_e`: total tree height.
    height: i32,
    /// `height`: trunk height, derived from `field_878_e`.
    trunk_height: i32,
    /// `field_870_m`: height range.
    height_range: i32,
    /// `field_869_n`: leaf cluster height.
    cluster_height: i32,
    /// `field_873_j`: branch length scale.
    branch_scale: f64,
    /// `field_872_k`: cluster density scale.
    cluster_scale: f64,
    clusters: Vec<[i32; 4]>,
}

impl BigTree {
    const TRUNK_SCALE: f64 = 0.618;
    const BRANCH_DROP: f64 = 0.381;

    /// A new generator after `func_517_a(height, branch, density)`.
    fn new(height_scale: f64, branch_scale: f64, cluster_scale: f64) -> Self {
        Self {
            random: JavaRandom::new(0),
            base: [0, 0, 0],
            height: 0,
            trunk_height: 0,
            height_range: (height_scale * 12.0) as i32,
            cluster_height: if height_scale > 0.5 { 5 } else { 4 },
            branch_scale,
            cluster_scale,
            clusters: Vec::new(),
        }
    }

    fn generate(
        &mut self,
        world: &mut PopulationWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
    ) -> bool {
        self.random = JavaRandom::new(rand.next_long() as u64);
        self.base = [x, y, z];
        if self.height == 0 {
            self.height = 5 + self.random.next_int(self.height_range as u32) as i32;
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
    fn can_grow(&mut self, world: &PopulationWorld) -> bool {
        let base = self.base;
        let top = [base[0], base[1] + self.height - 1, base[2]];
        if !grows_on(world.get(base[0], base[1] - 1, base[2])) {
            return false;
        }
        let distance = Self::line_clearance(world, base, top);
        if distance == -1 {
            true
        } else if distance < 6 {
            false
        } else {
            self.height = distance;
            true
        }
    }

    /// `func_524_a`: how far a line runs before it meets something other than
    /// air or leaves, or -1 when it is clear. Unlike [`Self::draw_line`], the
    /// minor axes are floored without rounding.
    fn line_clearance(world: &PopulationWorld, from: [i32; 3], to: [i32; 3]) -> i32 {
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
        let slope_a = f64::from(delta[a]) / f64::from(delta[axis]);
        let slope_b = f64::from(delta[b]) / f64::from(delta[axis]);
        let mut pos = [0i32; 3];
        let mut offset = 0i32;
        let end = delta[axis] + step;
        while offset != end {
            pos[axis] = from[axis] + offset;
            pos[a] = (f64::from(from[a]) + f64::from(offset) * slope_a).floor() as i32;
            pos[b] = (f64::from(from[b]) + f64::from(offset) * slope_b).floor() as i32;
            if !is_air_or_leaves(world.get(pos[0], pos[1], pos[2])) {
                break;
            }
            offset += step;
        }
        if offset == end { -1 } else { offset.abs() }
    }

    /// `func_521_a`: choose branch tips and the leaf clusters they carry.
    fn make_clusters(&mut self, world: &PopulationWorld) {
        self.trunk_height = (f64::from(self.height) * Self::TRUNK_SCALE) as i32;
        if self.trunk_height >= self.height {
            self.trunk_height = self.height - 1;
        }
        let mut cluster_count =
            (1.382 + (self.cluster_scale * f64::from(self.height) / 13.0).powf(2.0)) as i32;
        if cluster_count < 1 {
            cluster_count = 1;
        }

        let mut clusters = Vec::new();
        let mut level = self.base[1] + self.height - self.cluster_height;
        let top = self.base[1] + self.trunk_height;
        let mut remaining = level - self.base[1];
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
                let distance = self.branch_scale
                    * f64::from(radius)
                    * (f64::from(self.random.next_float()) + 0.328);
                let angle = f64::from(self.random.next_float()) * 2.0 * BRANCH_ANGLE_SCALE;
                let cx = (distance * angle.sin() + f64::from(self.base[0]) + 0.5).floor() as i32;
                let cz = (distance * angle.cos() + f64::from(self.base[2]) + 0.5).floor() as i32;
                let tip = [cx, level, cz];
                let tip_top = [cx, level + self.cluster_height, cz];
                if Self::line_clearance(world, tip, tip_top) != -1 {
                    continue;
                }
                let mut branch_base = self.base;
                let horizontal = (f64::from((self.base[0] - cx).abs()).powf(2.0)
                    + f64::from((self.base[2] - cz).abs()).powf(2.0))
                .sqrt();
                let drop = horizontal * Self::BRANCH_DROP;
                branch_base[1] = if f64::from(tip[1]) - drop > f64::from(top) {
                    top
                } else {
                    (f64::from(tip[1]) - drop) as i32
                };
                if Self::line_clearance(world, branch_base, tip) == -1 {
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
        if f64::from(level) < f64::from(self.height as f32) * 0.3 {
            return -1.618;
        }
        let half = self.height as f32 / 2.0;
        let offset = self.height as f32 / 2.0 - level as f32;
        let radius = if offset == 0.0 {
            half
        } else if offset.abs() >= half {
            0.0
        } else {
            (f64::from(half.abs()).powf(2.0) - f64::from(offset.abs()).powf(2.0)).sqrt() as f32
        };
        radius * 0.5
    }

    /// `func_526_b`: leaf blob radius within a cluster.
    fn blob_radius(&self, offset: i32) -> f32 {
        if (0..self.cluster_height).contains(&offset) {
            if offset != 0 && offset != self.cluster_height - 1 {
                3.0
            } else {
                2.0
            }
        } else {
            -1.0
        }
    }

    /// `func_518_b`: place a leaf blob at every cluster.
    fn place_clusters(&self, world: &mut PopulationWorld) {
        for cluster in &self.clusters {
            for level in cluster[1]..cluster[1] + self.cluster_height {
                let radius = self.blob_radius(level - cluster[1]);
                Self::place_leaf_disc(world, cluster[0], level, cluster[2], radius);
            }
        }
    }

    /// `func_523_a`: a horizontal disc of oak leaves. Any existing leaves,
    /// whatever their species, are replaced; everything else is kept.
    fn place_leaf_disc(world: &mut PopulationWorld, x: i32, y: i32, z: i32, radius: f32) {
        let extent = (f64::from(radius) + 0.618) as i32;
        for offset_x in -extent..=extent {
            for offset_z in -extent..=extent {
                let distance = ((f64::from(offset_x.abs()) + 0.5).powf(2.0)
                    + (f64::from(offset_z.abs()) + 0.5).powf(2.0))
                .sqrt();
                if distance > f64::from(radius) {
                    continue;
                }
                let (lx, lz) = (x + offset_x, z + offset_z);
                if is_air_or_leaves(world.get(lx, y, lz)) {
                    world.set(lx, y, lz, Block::Leaves);
                }
            }
        }
    }

    /// `func_529_c`: draw the trunk.
    fn place_trunk(&self, world: &mut PopulationWorld) {
        let base = self.base;
        let top = [base[0], base[1] + self.trunk_height, base[2]];
        Self::draw_line(world, base, top);
    }

    /// `func_525_d`: draw a branch from the trunk to each cluster.
    fn place_branches(&self, world: &mut PopulationWorld) {
        let mut from = self.base;
        for cluster in &self.clusters {
            let to = [cluster[0], cluster[1], cluster[2]];
            from[1] = cluster[3];
            let length = from[1] - self.base[1];
            if f64::from(length) >= f64::from(self.height) * 0.2 {
                Self::draw_line(world, from, to);
            }
        }
    }

    /// `func_522_a`: draw a line of wood between two points.
    fn draw_line(world: &mut PopulationWorld, from: [i32; 3], to: [i32; 3]) {
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
        let slope_a = f64::from(delta[a]) / f64::from(delta[axis]);
        let slope_b = f64::from(delta[b]) / f64::from(delta[axis]);
        let mut pos = [0i32; 3];
        let mut offset = 0i32;
        let end = delta[axis] + step;
        while offset != end {
            pos[axis] = (f64::from(from[axis] + offset) + 0.5).floor() as i32;
            pos[a] = (f64::from(from[a]) + f64::from(offset) * slope_a + 0.5).floor() as i32;
            pos[b] = (f64::from(from[b]) + f64::from(offset) * slope_b + 0.5).floor() as i32;
            world.set(pos[0], pos[1], pos[2], Block::Wood);
            offset += step;
        }
    }
}
