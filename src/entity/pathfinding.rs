//! Beta 1.7.3 `Pathfinder`: the A* search creatures use to wander and chase.
//!
//! This transcribes `Pathfinder`, its binary heap (`Path`), and `PathEntity`.
//! Nodes step only along the four horizontal axes. A node may climb one block
//! when the space above the current node is clear, and drops at most three
//! blocks. Every node must lie within `max_distance` of the target, and when
//! the target cannot be reached the walker heads for the closest node found.
//! Like `World.getEntityPathToXYZ`, the search only sees the chunks of a
//! bounded region around the walker; everything outside reads as air.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::block::blocks::Block;
use crate::block::fluids::is_lava;
use crate::block::fluids::is_water;
use crate::entity::EntitySize;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;

/// `PathEntity`: the points of a found path and the one being walked to.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    points: Vec<IVec3>,
    index: usize,
}

impl Path {
    pub fn new(points: Vec<IVec3>) -> Self {
        Self { points, index: 0 }
    }

    /// Every node from the walker's starting cell to the last reachable one.
    pub fn points(&self) -> &[IVec3] {
        &self.points
    }

    /// `PathEntity.incrementPathIndex`.
    pub fn advance(&mut self) {
        self.index += 1;
    }

    /// `PathEntity.isFinished`.
    pub fn is_finished(&self) -> bool {
        self.index >= self.points.len()
    }

    /// `PathEntity.getPosition`: where the walker's feet should go for the
    /// current node, centered on the cells its body covers.
    pub fn position(&self, width: f32) -> Vec3 {
        let point = self.points[self.index];
        let center = (width + 1.0) as i32 as f32 * 0.5;
        Vec3::new(
            point.x as f32 + center,
            point.y as f32,
            point.z as f32 + center,
        )
    }
}

/// One `PathPoint`. `heap` is its slot in the open heap, or `-1` when it is
/// not queued (`isAssigned`). `closed` is Beta's `isFirst`.
#[derive(Clone, Copy, Debug)]
struct Node {
    position: IVec3,
    heap: i32,
    total: f32,
    to_next: f32,
    to_target: f32,
    previous: Option<u32>,
    closed: bool,
}

/// How much searching a [`Pathfinder`] has done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStats {
    /// Searches run.
    pub searches: u64,
    /// Nodes taken off the open heap across those searches.
    pub nodes: u64,
    /// Searches answered from the walker's identical previous search.
    pub reused: u64,
}

impl SearchStats {
    pub fn add(&mut self, other: Self) {
        self.searches += other.searches;
        self.nodes += other.nodes;
        self.reused += other.reused;
    }
}

/// Everything a search reads: its start and end cells, the body's span, the
/// distance limit, and which chunks its region holds and their state. The
/// search draws no random numbers, so two searches with the same key find the
/// same path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SearchKey {
    start: IVec3,
    end: IVec3,
    span: IVec3,
    max_distance: u32,
    /// The region's corner chunks, `(min.x, min.z, max.x, max.z)`.
    region: [i32; 4],
    membership: u64,
    edits: u64,
}

/// One walker's last [`Pathfinder::path_to_feet_reusing`] search and the
/// path it found.
#[derive(Clone, Debug, Default)]
pub struct LastSearch(Option<(SearchKey, Option<Path>)>);

/// `Pathfinder` with its point map and heap kept between searches, so a
/// search does not allocate once the buffers have grown.
#[derive(Default)]
pub struct Pathfinder {
    nodes: Vec<Node>,
    lookup: HashMap<IVec3, u32>,
    heap: Vec<u32>,
    stats: SearchStats,
}

/// What `getVerticalOffset` found in the cells a body would occupy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Clearance {
    /// `1`: nothing in the way.
    Open,
    /// `0`: a solid material or a closed door.
    Blocked,
    /// `-1`: water, which a walker may enter but not fall through.
    Water,
    /// `-2`: lava, which it refuses to stand in.
    Lava,
}

impl Pathfinder {
    /// The searching done since the last call.
    pub fn take_stats(&mut self) -> SearchStats {
        std::mem::take(&mut self.stats)
    }

    /// `World.getEntityPathToXYZ`: path to the center of a block, as the
    /// wander AI uses.
    pub fn path_to_block(
        &mut self,
        chunks: &WorldChunks,
        feet: Vec3,
        size: EntitySize,
        target: IVec3,
        max_distance: f32,
    ) -> Option<Path> {
        let region = Region::new(chunks, feet, (max_distance + 8.0) as i32);
        self.search(
            &region,
            feet,
            size,
            target.as_vec3() + Vec3::splat(0.5),
            max_distance,
        )
    }

    /// `World.getPathToEntity`: path to another entity's feet, as chasing
    /// and following use.
    pub fn path_to_feet(
        &mut self,
        chunks: &WorldChunks,
        feet: Vec3,
        size: EntitySize,
        target: Vec3,
        max_distance: f32,
    ) -> Option<Path> {
        let region = Region::new(chunks, feet, (max_distance + 16.0) as i32);
        self.search(&region, feet, size, target, max_distance)
    }

    /// [`Self::path_to_feet`] for a walker that may ask the same question
    /// every tick, as a creature chasing the player does while it has no
    /// path. When nothing the search would read has changed since the
    /// walker's `last` search, that search's path is returned again instead of
    /// searching.
    pub fn path_to_feet_reusing(
        &mut self,
        chunks: &WorldChunks,
        feet: Vec3,
        size: EntitySize,
        target: Vec3,
        max_distance: f32,
        last: &mut LastSearch,
    ) -> Option<Path> {
        let region = Region::new(chunks, feet, (max_distance + 16.0) as i32);
        let (start, end, span) = cells(feet, size, target);
        let key = SearchKey {
            start,
            end,
            span,
            max_distance: max_distance.to_bits(),
            region: region.bounds,
            membership: region.membership,
            edits: region.edits,
        };
        if let Some((previous, path)) = &last.0
            && *previous == key
        {
            self.stats.reused += 1;
            return path.clone();
        }
        let path = self.search(&region, feet, size, target, max_distance);
        last.0 = Some((key, path.clone()));
        path
    }

    /// `createEntityPathTo` and `addToPath`.
    fn search(
        &mut self,
        region: &Region,
        feet: Vec3,
        size: EntitySize,
        target: Vec3,
        max_distance: f32,
    ) -> Option<Path> {
        self.nodes.clear();
        self.lookup.clear();
        self.heap.clear();
        self.stats.searches += 1;
        let (start, end, span) = cells(feet, size, target);
        let start = self.open(start);
        let end = self.open(end);

        let to_end = self.distance(start, end);
        let node = &mut self.nodes[start as usize];
        node.total = 0.0;
        node.to_next = to_end;
        node.to_target = to_end;
        self.push(start);
        let mut closest = start;
        let mut options = [0; 4];
        while !self.heap.is_empty() {
            let current = self.dequeue();
            self.stats.nodes += 1;
            if current == end {
                return Some(self.build(end));
            }
            if self.distance(current, end) < self.distance(closest, end) {
                closest = current;
            }
            self.nodes[current as usize].closed = true;
            let count = self.options(region, current, span, end, max_distance, &mut options);
            for &option in &options[..count] {
                let total = self.nodes[current as usize].total + self.distance(current, option);
                let queued = self.nodes[option as usize].heap >= 0;
                if !queued || total < self.nodes[option as usize].total {
                    let to_next = self.distance(option, end);
                    let node = &mut self.nodes[option as usize];
                    node.previous = Some(current);
                    node.total = total;
                    node.to_next = to_next;
                    if queued {
                        self.change_distance(option, total + to_next);
                    } else {
                        self.nodes[option as usize].to_target = total + to_next;
                        self.push(option);
                    }
                }
            }
        }
        (closest != start).then(|| self.build(closest))
    }

    /// `findPathOptions`.
    fn options(
        &mut self,
        region: &Region,
        current: u32,
        span: IVec3,
        target: u32,
        max_distance: f32,
        out: &mut [u32; 4],
    ) -> usize {
        let at = self.nodes[current as usize].position;
        let climb = i32::from(region.clearance(at + IVec3::Y, span) == Clearance::Open);
        let candidates = [
            self.safe_point(region, at + IVec3::Z, span, climb),
            self.safe_point(region, at - IVec3::X, span, climb),
            self.safe_point(region, at + IVec3::X, span, climb),
            self.safe_point(region, at - IVec3::Z, span, climb),
        ];
        let mut count = 0;
        for candidate in candidates.into_iter().flatten() {
            if !self.nodes[candidate as usize].closed
                && self.distance(candidate, target) < max_distance
            {
                out[count] = candidate;
                count += 1;
            }
        }
        count
    }

    /// `getSafePoint`: the cell to stand in after stepping into `at`, after
    /// an optional one-block climb and falling at most three blocks.
    fn safe_point(&mut self, region: &Region, at: IVec3, span: IVec3, climb: i32) -> Option<u32> {
        let IVec3 { x, mut y, z } = at;
        let mut point = None;
        if region.clearance(at, span) == Clearance::Open {
            point = Some(self.open(at));
        }
        if point.is_none()
            && climb > 0
            && region.clearance(IVec3::new(x, y + climb, z), span) == Clearance::Open
        {
            point = Some(self.open(IVec3::new(x, y + climb, z)));
            y += climb;
        }
        if point.is_some() {
            let mut fallen = 0;
            let mut below = Clearance::Blocked;
            while y > 0 && {
                below = region.clearance(IVec3::new(x, y - 1, z), span);
                below == Clearance::Open
            } {
                fallen += 1;
                if fallen >= 4 {
                    return None;
                }
                y -= 1;
                if y > 0 {
                    point = Some(self.open(IVec3::new(x, y, z)));
                }
            }
            if below == Clearance::Lava {
                return None;
            }
        }
        point
    }

    /// `openPoint`: the node at a cell, created on first use.
    fn open(&mut self, position: IVec3) -> u32 {
        if let Some(&index) = self.lookup.get(&position) {
            return index;
        }
        let index = self.nodes.len() as u32;
        self.nodes.push(Node {
            position,
            heap: -1,
            total: 0.0,
            to_next: 0.0,
            to_target: 0.0,
            previous: None,
            closed: false,
        });
        self.lookup.insert(position, index);
        index
    }

    /// `PathPoint.distanceTo`.
    fn distance(&self, from: u32, to: u32) -> f32 {
        let delta = self.nodes[to as usize].position - self.nodes[from as usize].position;
        delta.as_vec3().length()
    }

    /// `createEntityPath`: the chain of `previous` links back to the start.
    fn build(&self, end: u32) -> Path {
        let mut points = Vec::new();
        let mut node = Some(end);
        while let Some(index) = node {
            let node_data = self.nodes[index as usize];
            points.push(node_data.position);
            node = node_data.previous;
        }
        points.reverse();
        Path::new(points)
    }

    /// `Path.addPoint`.
    fn push(&mut self, node: u32) {
        let slot = self.heap.len();
        self.heap.push(node);
        self.nodes[node as usize].heap = slot as i32;
        self.sort_back(slot);
    }

    /// `Path.dequeue`.
    fn dequeue(&mut self) -> u32 {
        let first = self.heap.swap_remove(0);
        if !self.heap.is_empty() {
            self.sort_forward(0);
        }
        self.nodes[first as usize].heap = -1;
        first
    }

    /// `Path.changeDistance`.
    fn change_distance(&mut self, node: u32, distance: f32) {
        let previous = self.nodes[node as usize].to_target;
        self.nodes[node as usize].to_target = distance;
        let slot = self.nodes[node as usize].heap as usize;
        if distance < previous {
            self.sort_back(slot);
        } else {
            self.sort_forward(slot);
        }
    }

    /// `Path.sortBack`: sift a slot toward the root.
    fn sort_back(&mut self, mut slot: usize) {
        let node = self.heap[slot];
        let distance = self.nodes[node as usize].to_target;
        while slot > 0 {
            let parent = (slot - 1) >> 1;
            let parent_node = self.heap[parent];
            if distance >= self.nodes[parent_node as usize].to_target {
                break;
            }
            self.heap[slot] = parent_node;
            self.nodes[parent_node as usize].heap = slot as i32;
            slot = parent;
        }
        self.heap[slot] = node;
        self.nodes[node as usize].heap = slot as i32;
    }

    /// `Path.sortForward`: sift a slot toward the leaves.
    fn sort_forward(&mut self, mut slot: usize) {
        let node = self.heap[slot];
        let distance = self.nodes[node as usize].to_target;
        loop {
            let left = 1 + (slot << 1);
            let right = left + 1;
            if left >= self.heap.len() {
                break;
            }
            let left_node = self.heap[left];
            let left_distance = self.nodes[left_node as usize].to_target;
            let (right_node, right_distance) = match self.heap.get(right) {
                Some(&right_node) => (right_node, self.nodes[right_node as usize].to_target),
                None => (u32::MAX, f32::INFINITY),
            };
            let (child, child_node) = if left_distance < right_distance {
                if left_distance >= distance {
                    break;
                }
                (left, left_node)
            } else {
                if right_distance >= distance {
                    break;
                }
                (right, right_node)
            };
            self.heap[slot] = child_node;
            self.nodes[child_node as usize].heap = slot as i32;
            slot = child;
        }
        self.heap[slot] = node;
        self.nodes[node as usize].heap = slot as i32;
    }
}

/// The walker's start cell, the target's cell, and the cells the body spans.
fn cells(feet: Vec3, size: EntitySize, target: Vec3) -> (IVec3, IVec3, IVec3) {
    let start = size.aabb(feet).min.floor().as_ivec3();
    let end = IVec3::new(
        (target.x - size.width / 2.0).floor() as i32,
        target.y.floor() as i32,
        (target.z - size.width / 2.0).floor() as i32,
    );
    let span = IVec3::new(
        (size.width + 1.0).floor() as i32,
        (size.height + 1.0).floor() as i32,
        (size.width + 1.0).floor() as i32,
    );
    (start, end, span)
}

const INLINE_CHUNKS: usize = 25;

/// Beta's `ChunkCache`: the chunks within `radius` blocks of the walker, by
/// whole chunks. Blocks outside it, above or below the world, or in a chunk
/// that is not loaded read as air.
struct Region<'a> {
    min: ChunkPosition,
    width: i32,
    /// The region's chunks, row by row. A search of up to 16 blocks spans at
    /// most five chunks a side, so they are kept inline: a creature chasing
    /// the player builds a region every tick.
    inline: [Option<&'a Chunk>; INLINE_CHUNKS],
    /// The same for a region too large for `inline`, which is then unused.
    spill: Vec<Option<&'a Chunk>>,
    /// `(min.x, min.z, max.x, max.z)` in chunks.
    bounds: [i32; 4],
    /// Which chunks were loaded anywhere, and how many writes the region's
    /// chunks have taken. Together they change whenever anything the search
    /// could read does.
    membership: u64,
    edits: u64,
}

impl<'a> Region<'a> {
    fn new(chunks: &'a WorldChunks, feet: Vec3, radius: i32) -> Self {
        let center = feet.floor().as_ivec3();
        let min = ChunkPosition {
            x: (center.x - radius) >> 4,
            z: (center.z - radius) >> 4,
        };
        let max = ChunkPosition {
            x: (center.x + radius) >> 4,
            z: (center.z + radius) >> 4,
        };
        let width = max.x - min.x + 1;
        let count = (width * (max.z - min.z + 1)) as usize;
        let mut inline = [None; INLINE_CHUNKS];
        let mut spill = Vec::new();
        if count > INLINE_CHUNKS {
            spill.resize(count, None);
        }
        let slots = if spill.is_empty() {
            &mut inline[..]
        } else {
            &mut spill[..]
        };
        let mut edits = 0;
        let mut slot = 0;
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                let chunk = chunks.get(ChunkPosition { x, z }).map(|c| &c.chunk);
                edits += chunk.map_or(0, |chunk| u64::from(chunk.revision()));
                slots[slot] = chunk;
                slot += 1;
            }
        }
        Self {
            min,
            width,
            inline,
            spill,
            bounds: [min.x, min.z, max.x, max.z],
            membership: chunks.membership_revision(),
            edits,
        }
    }

    fn chunk(&self, x: i32, y: i32, z: i32) -> Option<(&'a Chunk, usize, usize, usize)> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return None;
        }
        let cx = (x >> 4) - self.min.x;
        let cz = (z >> 4) - self.min.z;
        if cx < 0 || cx >= self.width || cz < 0 {
            return None;
        }
        let slots = if self.spill.is_empty() {
            &self.inline[..]
        } else {
            &self.spill[..]
        };
        let chunk = (*slots.get((cz * self.width + cx) as usize)?)?;
        Some((chunk, (x & 15) as usize, y as usize, (z & 15) as usize))
    }

    fn block(&self, x: i32, y: i32, z: i32) -> Block {
        self.chunk(x, y, z)
            .and_then(|(chunk, x, y, z)| chunk.get(x, y, z))
            .unwrap_or(Block::Air)
    }

    fn metadata(&self, x: i32, y: i32, z: i32) -> u8 {
        self.chunk(x, y, z)
            .map_or(0, |(chunk, x, y, z)| chunk.metadata(x, y, z))
    }

    /// `getVerticalOffset`: scan the cells a `span`-sized body would occupy
    /// with its minimum corner at `at`, stopping at the first that decides.
    fn clearance(&self, at: IVec3, span: IVec3) -> Clearance {
        for x in at.x..at.x + span.x {
            for y in at.y..at.y + span.y {
                for z in at.z..at.z + span.z {
                    let block = self.block(x, y, z);
                    if block == Block::Air {
                        continue;
                    }
                    if matches!(block, Block::WoodenDoor | Block::IronDoor) {
                        // `BlockDoor.isOpen`.
                        if self.metadata(x, y, z) & 4 == 0 {
                            return Clearance::Blocked;
                        }
                    } else if block.is_solid_material() {
                        return Clearance::Blocked;
                    } else if is_water(block) {
                        return Clearance::Water;
                    } else if is_lava(block) {
                        return Clearance::Lava;
                    }
                }
            }
        }
        Clearance::Open
    }
}
