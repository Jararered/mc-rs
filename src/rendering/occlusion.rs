//! Cave culling: which render sections the camera could see through open
//! space.
//!
//! A mesh job records, for each 16³ section, which pairs of its six faces are
//! joined by a path through cells that are not opaque cubes
//! ([`SectionVisibility`]). Each time the camera enters another section,
//! [`cull_sections`] walks the loaded sections outward from it. A walk enters
//! a section through one face and leaves only through faces joined to one it
//! entered by, and it never steps back toward the camera along an axis. What
//! it reaches is [`VisibleSections`]; the rest is hidden behind terrain.
//!
//! The walk does not test the frustum, so it only has to run again when the
//! camera changes section or a section's graph changes, not when it turns.
//! Frustum culling still happens per layer or per region.
//!
//! A camera inside an opaque cube (flying through terrain) sees through
//! every wall, so culling is off while it is there.

use std::collections::HashMap;
use std::sync::LazyLock;

use bevy::prelude::*;

use crate::block::blocks::Block;
use crate::player::PlayerCamera;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;
use crate::world::chunk::SECTIONS_PER_CHUNK;
use crate::world::chunk::WorldChunks;
use crate::world::streaming::WorldStreaming;

/// Set to `off` to draw every section the frustum holds.
pub const OCCLUSION_ENV: &str = "MC_OCCLUSION";

/// Section faces in `geometry::FACE_*` order: up, down, east (+x), west (−x),
/// south (+z), north (−z). Opposite faces differ in the lowest bit.
pub const DIRECTIONS: usize = 6;
const STEPS: [[i32; 3]; DIRECTIONS] = [
    [0, 1, 0],
    [0, -1, 0],
    [1, 0, 0],
    [-1, 0, 0],
    [0, 0, 1],
    [0, 0, -1],
];
const ALL_DIRECTIONS: u8 = (1 << DIRECTIONS) - 1;
const UP: usize = 0;
const DOWN: usize = 1;

const SECTION_CELLS: usize = CHUNK_SIZE * SECTION_HEIGHT * CHUNK_SIZE;
/// Fewer opaque cells than one full layer cannot wall off any face from
/// another, so the flood fill is skipped.
const WALL_CELLS: usize = CHUNK_SIZE * CHUNK_SIZE;

/// Which faces of a section see each other: bit `from * 8 + to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionVisibility(u64);

impl Default for SectionVisibility {
    /// A section not meshed yet hides nothing.
    fn default() -> Self {
        Self::ALL
    }
}

impl SectionVisibility {
    pub const ALL: Self = Self(0x3f3f_3f3f_3f3f);
    pub const NONE: Self = Self(0);

    /// Whether a path through open cells joins faces `from` and `to`.
    pub fn connects(self, from: usize, to: usize) -> bool {
        self.0 >> (from * 8 + to) & 1 != 0
    }

    /// Faces reachable from any face in `entered`.
    pub fn exits(self, entered: u8) -> u8 {
        (0..DIRECTIONS)
            .filter(|from| entered >> from & 1 != 0)
            .fold(0, |exits, from| exits | (self.0 >> (from * 8)) as u8)
            & ALL_DIRECTIONS
    }

    /// Flood-fill section `section` of `chunk` through every cell that is not
    /// an opaque cube.
    pub fn compute(chunk: &Chunk, section: usize) -> Self {
        let start = Chunk::index(0, section * SECTION_HEIGHT, 0);
        let blocks = &chunk.raw_blocks()[start..start + SECTION_CELLS];
        let opaque = &*OPAQUE_CUBES;
        let solid = blocks
            .iter()
            .filter(|raw| opaque[usize::from(**raw)])
            .count();
        if solid < WALL_CELLS {
            return Self::ALL;
        }
        if solid == SECTION_CELLS {
            return Self::NONE;
        }
        // Opaque cells start out visited, so the fill never enters them.
        let mut visited: Vec<bool> = blocks.iter().map(|raw| opaque[usize::from(*raw)]).collect();
        let mut stack = Vec::with_capacity(SECTION_CELLS);
        let mut visibility = 0u64;
        for seed in 0..SECTION_CELLS {
            if visited[seed] {
                continue;
            }
            visited[seed] = true;
            stack.push(seed);
            let mut faces = 0u8;
            while let Some(cell) = stack.pop() {
                let x = cell % CHUNK_SIZE;
                let z = cell / CHUNK_SIZE % CHUNK_SIZE;
                let y = cell / (CHUNK_SIZE * CHUNK_SIZE);
                for (direction, [dx, dy, dz]) in STEPS.into_iter().enumerate() {
                    let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                    let size = CHUNK_SIZE as i32;
                    if nx < 0
                        || nx >= size
                        || nz < 0
                        || nz >= size
                        || ny < 0
                        || ny >= SECTION_HEIGHT as i32
                    {
                        faces |= 1 << direction;
                        continue;
                    }
                    let next = (ny as usize * CHUNK_SIZE + nz as usize) * CHUNK_SIZE + nx as usize;
                    if !visited[next] {
                        visited[next] = true;
                        stack.push(next);
                    }
                }
            }
            for from in 0..DIRECTIONS {
                if faces >> from & 1 != 0 {
                    visibility |= u64::from(faces) << (from * 8);
                }
            }
        }
        Self(visibility)
    }
}

static OPAQUE_CUBES: LazyLock<[bool; 256]> =
    LazyLock::new(|| std::array::from_fn(|raw| Block::from(raw as u8).is_opaque_cube()));

/// One bit per section of a chunk, bottom first, as in streaming's masks.
pub type VisibleMask = u8;

/// The sections [`cull_sections`] last reached, by chunk.
#[derive(Resource, Debug)]
pub struct VisibleSections {
    enabled: bool,
    columns: HashMap<ChunkPosition, VisibleMask>,
    /// Whether `columns` holds a walk's result.
    walked: bool,
    /// What the visible set was last computed for.
    walked_for: Option<Viewpoint>,
    reached: usize,
    /// Dense scratch buffers, retained across walks. The traversal indexes
    /// sections directly rather than hashing a chunk for every visit.
    graph: Vec<SectionVisibility>,
    entered: Vec<u8>,
    queue: Vec<IVec3>,
    masks: Vec<VisibleMask>,
}

/// Where the camera was when the visible set was last computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Viewpoint {
    /// Inside an opaque cube, where nothing is culled.
    Inside,
    /// In this section, over this graph revision.
    At(IVec3, u64),
}

impl Default for VisibleSections {
    fn default() -> Self {
        Self::new(!std::env::var(OCCLUSION_ENV).is_ok_and(|value| value == "off"))
    }
}

impl VisibleSections {
    /// Culling turned on or off. Off reports every section visible.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            columns: HashMap::new(),
            walked: false,
            walked_for: None,
            reached: 0,
            graph: Vec::new(),
            entered: Vec::new(),
            queue: Vec::new(),
            masks: Vec::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The sections of `position` that may be visible.
    pub fn mask(&self, position: ChunkPosition) -> VisibleMask {
        if !self.enabled || !self.walked {
            return VisibleMask::MAX;
        }
        self.columns.get(&position).copied().unwrap_or(0)
    }

    pub fn is_visible(&self, position: ChunkPosition, section: usize) -> bool {
        self.mask(position) >> section & 1 != 0
    }

    /// Forget the last walk, so every section is visible.
    fn clear(&mut self) {
        if self.walked || self.walked_for.is_some() {
            self.columns.clear();
            self.walked = false;
            self.walked_for = None;
            self.reached = 0;
        }
    }

    /// Sections the last walk reached.
    pub fn reached(&self) -> usize {
        self.reached
    }

    /// Walk outward from the camera at `camera` over the sections `graph`
    /// lists. Sections of chunks missing from `graph` but inside its bounds
    /// are open: an unmeshed chunk draws nothing and hides nothing.
    pub fn walk<'a>(
        &mut self,
        camera: Vec3,
        graph: impl Iterator<Item = (ChunkPosition, &'a [SectionVisibility; SECTIONS_PER_CHUNK])>,
    ) {
        self.columns.clear();
        self.reached = 0;
        self.walked = true;
        let graph: Vec<_> = graph.collect();
        let Some(bounds) = Bounds::of(graph.iter().map(|(position, _)| *position)) else {
            return;
        };
        let origin = camera_section(camera);
        self.graph.clear();
        self.graph.resize(bounds.cells(), SectionVisibility::ALL);
        for (position, sections) in graph {
            let start = bounds.index(IVec3::new(position.x, 0, position.z));
            self.graph[start..start + SECTIONS_PER_CHUNK].copy_from_slice(sections);
        }
        self.entered.clear();
        self.entered.resize(bounds.cells(), 0);
        self.masks.clear();
        self.masks.resize(bounds.cells() / SECTIONS_PER_CHUNK, 0);
        self.queue.clear();
        let entered = &mut self.entered;
        let queue = &mut self.queue;
        let top = SECTIONS_PER_CHUNK as i32 - 1;
        if (0..=top).contains(&origin.y) {
            let column = IVec3::new(
                origin.x.clamp(bounds.min.x, bounds.max.x),
                origin.y,
                origin.z.clamp(bounds.min.y, bounds.max.y),
            );
            entered[bounds.index(column)] = ALL_DIRECTIONS;
            queue.push(column);
        } else {
            // Above or below the world: every column's end section is open
            // to the camera through its outer face.
            let (y, face) = if origin.y > top { (top, UP) } else { (0, DOWN) };
            for z in bounds.min.y..=bounds.max.y {
                for x in bounds.min.x..=bounds.max.x {
                    let cell = IVec3::new(x, y, z);
                    entered[bounds.index(cell)] = 1 << face;
                    queue.push(cell);
                }
            }
        }
        let mut next = 0;
        while let Some(&cell) = queue.get(next) {
            next += 1;
            let index = bounds.index(cell);
            self.masks[index / SECTIONS_PER_CHUNK] |= 1 << cell.y;
            self.reached += 1;
            let came_in = entered[index];
            let exits = if came_in == ALL_DIRECTIONS {
                ALL_DIRECTIONS
            } else {
                self.graph[index].exits(came_in)
            };
            for (direction, step) in STEPS.into_iter().enumerate() {
                if exits >> direction & 1 == 0 || !outward(origin, cell, step) {
                    continue;
                }
                let neighbor = cell + IVec3::from(step);
                if !(0..=top).contains(&neighbor.y) || !bounds.holds(neighbor) {
                    continue;
                }
                let slot = &mut entered[bounds.index(neighbor)];
                if *slot == 0 {
                    queue.push(neighbor);
                }
                // Entered through the face opposite the step.
                *slot |= 1 << (direction ^ 1);
            }
        }
        for (column, &mask) in self.masks.iter().enumerate() {
            if mask != 0 {
                self.columns.insert(
                    ChunkPosition {
                        x: bounds.min.x + (column % bounds.width()) as i32,
                        z: bounds.min.y + (column / bounds.width()) as i32,
                    },
                    mask,
                );
            }
        }
    }
}

/// Whether stepping `step` from `cell` does not bring it closer to the
/// camera's section along that axis.
fn outward(origin: IVec3, cell: IVec3, step: [i32; 3]) -> bool {
    (0..3).all(|axis| match step[axis] {
        1 => cell[axis] >= origin[axis],
        -1 => cell[axis] <= origin[axis],
        _ => true,
    })
}

/// Chunk x, section y, chunk z of a world position.
pub fn camera_section(camera: Vec3) -> IVec3 {
    let size = CHUNK_SIZE as f32;
    IVec3::new(
        (camera.x / size).floor() as i32,
        // Anything at or above the build limit is "above the world".
        if camera.y >= CHUNK_HEIGHT as f32 {
            SECTIONS_PER_CHUNK as i32
        } else {
            (camera.y / SECTION_HEIGHT as f32).floor() as i32
        },
        (camera.z / size).floor() as i32,
    )
}

/// The chunk columns a walk covers, `min` and `max` inclusive as (x, z).
struct Bounds {
    min: IVec2,
    max: IVec2,
}

impl Bounds {
    fn of(positions: impl Iterator<Item = ChunkPosition>) -> Option<Self> {
        positions.fold(None, |bounds: Option<Self>, position| {
            let at = IVec2::new(position.x, position.z);
            Some(match bounds {
                Some(bounds) => Self {
                    min: bounds.min.min(at),
                    max: bounds.max.max(at),
                },
                None => Self { min: at, max: at },
            })
        })
    }

    fn width(&self) -> usize {
        (self.max.x - self.min.x + 1) as usize
    }

    fn cells(&self) -> usize {
        self.width() * (self.max.y - self.min.y + 1) as usize * SECTIONS_PER_CHUNK
    }

    fn holds(&self, cell: IVec3) -> bool {
        (self.min.x..=self.max.x).contains(&cell.x) && (self.min.y..=self.max.y).contains(&cell.z)
    }

    fn index(&self, cell: IVec3) -> usize {
        let column = (cell.z - self.min.y) as usize * self.width() + (cell.x - self.min.x) as usize;
        column * SECTIONS_PER_CHUNK + cell.y as usize
    }
}

/// Walk again when the camera enters another section or a section's graph
/// changes.
pub fn cull_sections(
    mut visible: ResMut<VisibleSections>,
    streaming: Option<Res<WorldStreaming>>,
    chunks: Option<Res<WorldChunks>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    if !visible.enabled {
        return;
    }
    let (Some(streaming), Ok(camera)) = (streaming, camera.single()) else {
        visible.clear();
        return;
    };
    let eye = camera.translation();
    let inside = chunks.is_some_and(|chunks| {
        let cell = eye.floor().as_ivec3();
        chunks
            .block_at(cell.x, cell.y, cell.z)
            .is_some_and(Block::is_opaque_cube)
    });
    let key = if inside {
        Viewpoint::Inside
    } else {
        Viewpoint::At(camera_section(eye), streaming.graph_revision())
    };
    if visible.walked_for == Some(key) {
        return;
    }
    if inside {
        visible.clear();
    } else {
        visible.walk(eye, streaming.section_graph());
    }
    visible.walked_for = Some(key);
}
