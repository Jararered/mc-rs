//! The per-chunk mesh pass: walks a section's blocks and emits each one's
//! faces, fluids, and custom shapes.

use super::ChunkMeshes;
use super::ChunkNeighbors;
use super::ColumnTints;
use super::CornerShading;
use super::DEFAULT_GRASS_TINT;
use super::FACES;
use super::Face;
use super::GRASS_OVERLAY_TILE;
use super::LAYER_CUTOUT;
use super::LAYER_MASKED;
use super::LAYER_OPAQUE;
use super::LAYER_OVERLAY;
use super::LAYER_WATER;
use super::WATER_TINT;
use super::empty_meshes;
use super::faces::block_tint;
use super::faces::box_shape_masked;
use super::faces::box_texels;
use super::faces::chest_geometry;
use super::faces::chest_pair_direction;
use super::faces::double_chest_tile;
use super::faces::face_corner_ao;
use super::faces::face_corner_light;
use super::faces::face_texels;
use super::faces::fluid_shell_face_visible;
use super::faces::neighbor_hides_face_at;
use super::faces::snow_above;
use super::fluid_top_texels;
use super::geometry::BlockFaceGeometry;
use super::geometry::FACE_BOTTOM;
use super::geometry::FACE_EAST;
use super::geometry::FACE_NORTH;
use super::geometry::FACE_SOUTH;
use super::geometry::FACE_TOP;
use super::geometry::FACE_WEST;
use super::geometry::block_boxes;
use super::geometry::is_box_shape;
use super::planes::Plane;
use super::planes::flush_planes;
use super::planes::insert_plane;
use super::planes::layer_mut;
use super::planes::plane_coords;
use super::planes::plane_key;
use super::planes::quantized_tint;
use super::shaped_block;
use super::uniform_shading;
use super::vertex::FULL_BRIGHT;
use crate::block::blocks::Block;
use crate::block::fluids::Fluid;
use crate::block::fluids::corner_height;
use crate::rendering::textures::LAVA_FLOW_TILE;
use crate::rendering::textures::LAVA_STILL_TILE;
use crate::rendering::textures::SNOWY_GRASS_SIDE_TILE;
use crate::rendering::textures::WATER_FLOW_TILE;
use crate::rendering::textures::WATER_STILL_TILE;
use crate::rendering::textures::block_tile;
use crate::rendering::textures::door_tile;
use crate::rendering::textures::farmland_top_tile;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::SECTION_HEIGHT;
use crate::world::lighting::Skylight;
use std::ops::Range;

pub(super) struct Mesher<'a> {
    pub(super) chunk: &'a Chunk,
    pub(super) neighbors: &'a ChunkNeighbors<'a>,
    pub(super) skylight: &'a Skylight,
    pub(super) tints: Option<ColumnTints>,
    pub(super) fancy_graphics: bool,
    pub(super) origin_x: i32,
    pub(super) origin_z: i32,
    /// Omit every other block. Face culling still reads the real neighbors.
    pub(super) only: Option<Block>,
}

impl<'a> Mesher<'a> {
    pub(super) fn new(
        chunk: &'a Chunk,
        neighbors: &'a ChunkNeighbors<'a>,
        skylight: &'a Skylight,
        tints: Option<ColumnTints>,
        fancy_graphics: bool,
        origin_x: i32,
        origin_z: i32,
        only: Option<Block>,
    ) -> Self {
        Self {
            chunk,
            neighbors,
            skylight,
            tints,
            fancy_graphics,
            origin_x,
            origin_z,
            only,
        }
    }

    /// `RenderBlocks.renderBlockFluids`: a surface whose corners follow the
    /// levels around them, sides where the neighbor is open, and a bottom
    /// over open space. Water goes in the translucent layer, lava is opaque.
    pub(super) fn push_fluid(
        &self,
        meshes: &mut ChunkMeshes,
        origin: [f32; 3],
        x: usize,
        y: usize,
        z: usize,
        fluid: Fluid,
    ) {
        let chunk = self.chunk;
        let neighbors = self.neighbors;
        let (xi, yi, zi) = (x as i32, y as i32, z as i32);
        let cell = |cx: i32, cy: i32, cz: i32| neighbors.cell(chunk, cx, cy, cz);
        // `BlockFluid.shouldSideBeRendered`: never against the same fluid or
        // ice; the top whenever it is open to something else; other sides
        // unless an opaque cube covers them.
        let renders = |face: &Face| {
            let [dx, dy, dz] = face.neighbor;
            if yi + dy < 0 {
                return false;
            }
            match neighbors.get(chunk, xi + dx, yi + dy, zi + dz) {
                None => true,
                Some(neighbor) => {
                    Fluid::of(neighbor) != Some(fluid)
                        && neighbor != Block::Ice
                        && (dy > 0 || !neighbor.is_opaque_cube())
                }
            }
        };
        // Most water sits inside a lake with every face hidden.
        let visible = FACES.each_ref().map(|face| renders(face));
        if !visible.contains(&true) {
            return;
        }
        let height = |cx: i32, cz: i32| corner_height(fluid, cx, yi, cz, cell);
        let heights = [
            [height(xi, zi), height(xi, zi + 1)],
            [height(xi + 1, zi), height(xi + 1, zi + 1)],
        ];
        let corner_top = |corner: [f32; 3]| heights[corner[0] as usize][corner[2] as usize];
        let (still, flow, tint, layer) = match fluid {
            Fluid::Water => (
                WATER_STILL_TILE,
                WATER_FLOW_TILE,
                WATER_TINT,
                &mut meshes.water,
            ),
            Fluid::Lava => (
                LAVA_STILL_TILE,
                LAVA_FLOW_TILE,
                [1.0; 3],
                &mut meshes.opaque,
            ),
        };
        let unit_cube = BlockFaceGeometry::unit_cube();
        for (face_index, face) in FACES.iter().enumerate() {
            if !visible[face_index] {
                continue;
            }
            let unit = unit_cube.face(face_index);
            // Shade from the unit face: the light sampling side of a corner
            // must not flip for a low surface.
            let shading = CornerShading {
                light: face_corner_light(self.skylight, x, y, z, face, unit),
                ao: face_corner_ao(chunk, neighbors, x, y, z, face, unit),
                shade: true,
            };
            let corners = unit.corners.map(|corner| {
                let top = if corner[1] > 0.5 {
                    corner_top(corner)
                } else {
                    0.0
                };
                [corner[0], top, corner[2]]
            });
            let texels = if face_index == FACE_TOP {
                fluid_top_texels(fluid, xi, yi, zi, still, flow, &cell)
            } else if face_index == FACE_BOTTOM {
                face_texels(still.0, still.1, face_index)
            } else {
                // Sides show the flowing tile cut off at the surface.
                let mut texels = face_texels(flow.0, flow.1, face_index);
                for (texel, corner) in texels.iter_mut().zip(corners) {
                    if texel.texel[1] == 0 {
                        texel.texel[1] = ((1.0 - corner[1]) * 16.0).round() as u8;
                    }
                }
                texels
            };
            layer.push_block_quad(origin, face.normal, corners, texels, tint, shading);
        }
    }

    /// Mesh the chunk layers in `rows`, with positions relative to `y_origin`.
    ///
    /// Full cubes are merged inside each 16-block band. A section mesh is one
    /// band, so a rectangle never crosses a section boundary.
    pub(super) fn region(&self, rows: Range<usize>, y_origin: usize) -> ChunkMeshes {
        let mut meshes = empty_meshes();
        let mut start = rows.start;
        while start < rows.end {
            let end = (start + SECTION_HEIGHT).min(rows.end);
            self.mesh_band(&mut meshes, start..end, y_origin);
            start = end;
        }
        meshes
    }

    pub(super) fn mesh_band(&self, meshes: &mut ChunkMeshes, rows: Range<usize>, y_origin: usize) {
        let band_start = rows.start;
        let mut planes = std::array::from_fn(|_| Vec::<Plane>::new());
        let chunk = self.chunk;
        let skylight = self.skylight;
        for y in rows {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let block = chunk.get(x, y, z).unwrap();
                    if block == Block::Air || self.only.is_some_and(|only| block != only) {
                        continue;
                    }
                    let origin = [x as f32, (y - y_origin) as f32, z as f32];
                    let metadata = chunk.metadata(x, y, z);
                    if block.is_torch()
                        || matches!(block, Block::RedstoneTorch | Block::UnlitRedstoneTorch)
                    {
                        meshes.grass_overlay.push_torch(origin, block, metadata);
                        continue;
                    }
                    if block == Block::RedstoneWire {
                        self.push_redstone_wire(&mut meshes.masked, origin, x, y, z);
                        continue;
                    }
                    if matches!(block, Block::Repeater | Block::PoweredRepeater) {
                        self.push_repeater(&mut meshes.masked, origin, x, y, z, block);
                        continue;
                    }
                    if matches!(
                        block,
                        Block::Rail | Block::PoweredRail | Block::DetectorRail
                    ) {
                        self.push_rail(&mut meshes.masked, origin, x, y, z, block);
                        continue;
                    }
                    if block == Block::Lever {
                        self.push_lever(&mut meshes.masked, origin, x, y, z);
                        continue;
                    }
                    if matches!(
                        block,
                        Block::Piston | Block::StickyPiston | Block::PistonHead
                    ) {
                        self.push_piston(&mut meshes.opaque, origin, x, y, z, block);
                        continue;
                    }
                    if block == Block::MovingPiston {
                        continue;
                    }
                    if block.is_ladder() {
                        meshes.masked.push_ladder(origin, metadata);
                        continue;
                    }
                    if block == Block::NetherPortal {
                        let portal = |dx: i32| {
                            self.neighbors.get(chunk, x as i32 + dx, y as i32, z as i32)
                                == Some(Block::NetherPortal)
                        };
                        // Translucent, so it shares water's blended layer.
                        meshes.water.push_portal(origin, portal(-1) || portal(1));
                        continue;
                    }
                    let column = z * CHUNK_SIZE + x;
                    let grass_tint = self
                        .tints
                        .as_ref()
                        .map_or(DEFAULT_GRASS_TINT, |tints| tints.grass[column]);
                    if let Some(fluid) = Fluid::of(block) {
                        // A wireframe filter draws the open fluid surface as full
                        // cubes so it can share the greedy planes. Faces against
                        // opaque blocks stay on those blocks. Gameplay water
                        // keeps its sloped, per-cell surface.
                        if self.only == Some(block) {
                            self.queue_cube(
                                meshes,
                                &mut planes,
                                band_start,
                                y_origin,
                                [x, y, z],
                                block,
                                metadata,
                                grass_tint,
                            );
                        } else {
                            self.push_fluid(meshes, origin, x, y, z, fluid);
                        }
                        continue;
                    }
                    if block == Block::Crops {
                        let light = skylight.channels_at(x as i32, y as i32, z as i32);
                        meshes
                            .masked
                            .push_crops(origin, chunk.metadata(x, y, z), light);
                        continue;
                    }
                    if block == Block::Fire {
                        self.push_fire(&mut meshes.masked, origin, x, y, z);
                        continue;
                    }
                    if block.is_crossed_plant() || block == Block::Cobweb {
                        let light = skylight.channels_at(x as i32, y as i32, z as i32);
                        meshes.masked.push_crossed_plant(
                            origin,
                            [self.origin_x + x as i32, y as i32, self.origin_z + z as i32],
                            block,
                            metadata,
                            grass_tint,
                            light,
                        );
                        continue;
                    }
                    if block == Block::Bed {
                        self.push_bed(&mut meshes.masked, origin, x, y, z);
                        continue;
                    }
                    if is_box_shape(block) {
                        self.emit_boxes(meshes, origin, [x, y, z], block, metadata);
                        continue;
                    }
                    if shaped_block(block) {
                        self.emit_shaped(meshes, origin, [x, y, z], block, metadata, grass_tint);
                        continue;
                    }
                    self.queue_cube(
                        meshes,
                        &mut planes,
                        band_start,
                        y_origin,
                        [x, y, z],
                        block,
                        metadata,
                        grass_tint,
                    );
                }
            }
        }
        flush_planes(meshes, planes, band_start, y_origin);
    }

    /// Slabs, stairs, fences, doors, and trapdoors: one or more boxes inside
    /// the cell, textured by position as `renderStandardBlock` does for a
    /// block with custom bounds. Only a face on the cell boundary can be
    /// hidden by the neighbor.
    pub(super) fn emit_boxes(
        &self,
        meshes: &mut ChunkMeshes,
        origin: [f32; 3],
        at: [usize; 3],
        block: Block,
        metadata: u8,
    ) {
        let [x, y, z] = at;
        let chunk = self.chunk;
        let neighbors = self.neighbors;
        let skylight = self.skylight;
        let neighbor_at = |offset: [i32; 3]| {
            let ny = y as i32 + offset[1];
            (ny >= 0)
                .then(|| neighbors.get(chunk, x as i32 + offset[0], ny, z as i32 + offset[2]))
                .flatten()
        };
        let links = [[-1, 0, 0], [1, 0, 0], [0, 0, -1], [0, 0, 1]]
            .map(|offset| block == Block::Fence && neighbor_at(offset) == Some(Block::Fence));
        let layer = if box_shape_masked(block) {
            &mut meshes.masked
        } else {
            &mut meshes.opaque
        };
        let tint = block_tint(block, metadata, None);
        // A block that lets light through lights its inner faces from its
        // own cell, as `renderBlockDoor` does. Slabs and stairs hold no light.
        let own_light = (block.light_opacity() == 0)
            .then(|| [[skylight.channels_at(x as i32, y as i32, z as i32); 4]; 4]);
        for bounds in block_boxes(block, metadata, links).as_slice() {
            let geometry = BlockFaceGeometry::from_bounds(*bounds);
            for (face_index, face) in FACES.iter().enumerate() {
                let on_boundary = match face_index {
                    FACE_TOP => bounds[4] >= 1.0,
                    FACE_BOTTOM => bounds[1] <= 0.0,
                    FACE_EAST => bounds[3] >= 1.0,
                    FACE_WEST => bounds[0] <= 0.0,
                    FACE_SOUTH => bounds[5] >= 1.0,
                    _ => bounds[2] <= 0.0,
                };
                if on_boundary {
                    if y as i32 + face.neighbor[1] < 0 {
                        continue;
                    }
                    let [dx, dy, dz] = face.neighbor;
                    if neighbor_hides_face_at(
                        block,
                        neighbors,
                        chunk,
                        (x as i32 + dx, y as i32 + dy, z as i32 + dz),
                        face_index,
                        self.fancy_graphics,
                    ) {
                        continue;
                    }
                }
                let face_geometry = geometry.face(face_index);
                let (tile, flip) = if block.is_door() {
                    door_tile(block, metadata, face_index)
                } else {
                    (
                        block_tile(block, metadata, face_index, self.fancy_graphics),
                        false,
                    )
                };
                let shading = CornerShading {
                    light: match own_light {
                        Some(light) if !on_boundary => light,
                        _ => face_corner_light(skylight, x, y, z, face, face_geometry),
                    },
                    ao: if on_boundary {
                        face_corner_ao(chunk, neighbors, x, y, z, face, face_geometry)
                    } else {
                        [0; 4]
                    },
                    shade: true,
                };
                layer.push_block_quad(
                    origin,
                    face.normal,
                    face_geometry.corners,
                    box_texels(tile, face_index, face_geometry.corners, flip),
                    tint,
                    shading,
                );
            }
        }
    }

    /// Cactus, farmland, and chests keep per-face geometry.
    pub(super) fn emit_shaped(
        &self,
        meshes: &mut ChunkMeshes,
        origin: [f32; 3],
        at: [usize; 3],
        block: Block,
        metadata: u8,
        grass_tint: [f32; 3],
    ) {
        let [x, y, z] = at;
        let chunk = self.chunk;
        let neighbors = self.neighbors;
        let skylight = self.skylight;
        let fancy_graphics = self.fancy_graphics;
        let column = z * CHUNK_SIZE + x;
        let chest_pair = if block.is_chest() {
            chest_pair_direction(chunk, neighbors, x, y, z)
        } else {
            None
        };
        let block_geometry = if block.is_chest() {
            chest_geometry(chest_pair)
        } else {
            BlockFaceGeometry::for_block(block)
        };
        for (face_index, face) in FACES.iter().enumerate() {
            let face_geometry = block_geometry.face(face_index);
            if chest_pair == Some(face.neighbor) {
                continue;
            }
            let nx = x as i32 + face.neighbor[0];
            let ny = y as i32 + face.neighbor[1];
            let nz = z as i32 + face.neighbor[2];
            if ny < 0 {
                continue;
            }
            if neighbor_hides_face_at(
                block,
                neighbors,
                chunk,
                (nx, ny, nz),
                face_index,
                fancy_graphics,
            ) {
                continue;
            }
            let grass_side =
                block == Block::Grass && face_index != FACE_TOP && face_index != FACE_BOTTOM;
            let base = if block == Block::Grass && face_index == FACE_TOP {
                grass_tint
            } else {
                block_tint(
                    block,
                    metadata,
                    self.tints.as_ref().map(|tints| tints.foliage[column]),
                )
            };
            let layer = if block == Block::Cactus {
                &mut meshes.masked
            } else if fancy_graphics && block.is_leaves() {
                &mut meshes.cutout
            } else {
                &mut meshes.opaque
            };
            let chest_tile = chest_pair
                .map(|direction| double_chest_tile(block, metadata, direction, face_index));
            let wet_farmland_top =
                block == Block::Farmland && face_index == FACE_TOP && metadata > 0;
            let tile = wet_farmland_top
                .then(|| farmland_top_tile(true))
                .or(chest_tile)
                .unwrap_or_else(|| block_tile(block, metadata, face_index, fancy_graphics));
            let shading = CornerShading {
                light: face_corner_light(skylight, x, y, z, face, face_geometry),
                ao: face_corner_ao(chunk, neighbors, x, y, z, face, face_geometry),
                shade: true,
            };
            let side_tint = if grass_side { [1.0; 3] } else { base };
            if block.is_chest() {
                // The latch and the two halves of a double chest only line
                // up when each side reads the way Beta draws it.
                let mut texels = face_texels(tile.0, tile.1, face_index);
                if face_index == FACE_EAST || face_index == FACE_NORTH {
                    for texel in &mut texels {
                        texel.texel[0] = 16 - texel.texel[0];
                    }
                }
                layer.push_block_quad(
                    origin,
                    face.normal,
                    face_geometry.corners,
                    texels,
                    side_tint,
                    shading,
                );
                continue;
            }
            layer.push_face(
                origin,
                face,
                face_geometry,
                face_index,
                side_tint,
                shading,
                block,
                tile,
            );
            if grass_side && fancy_graphics {
                meshes
                    .grass_overlay
                    .push_grass_overlay(origin, face, face_index, grass_tint, shading);
            }
        }
    }

    /// Full unit-cube faces. Flat shading joins a bit plane; a corner gradient
    /// stays a single quad so smooth light is unchanged.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn queue_cube(
        &self,
        meshes: &mut ChunkMeshes,
        planes: &mut [Vec<Plane>; 6],
        band_start: usize,
        y_origin: usize,
        at: [usize; 3],
        block: Block,
        metadata: u8,
        grass_tint: [f32; 3],
    ) {
        let [x, y, z] = at;
        let origin = [x as f32, (y - y_origin) as f32, z as f32];
        let chunk = self.chunk;
        let neighbors = self.neighbors;
        let skylight = self.skylight;
        let fancy_graphics = self.fancy_graphics;
        let column = z * CHUNK_SIZE + x;
        let foliage = self.tints.as_ref().map(|tints| tints.foliage[column]);
        let unit = BlockFaceGeometry::unit_cube();
        for (face_index, face) in FACES.iter().enumerate() {
            let ny = y as i32 + face.neighbor[1];
            if ny < 0 {
                continue;
            }
            let neighbor = neighbors.get(
                chunk,
                x as i32 + face.neighbor[0],
                ny,
                z as i32 + face.neighbor[2],
            );
            // A filtered fluid keeps the open surface only. Faces against
            // sand, dirt, and other opaque cubes belong to those blocks.
            let filtered_fluid = (self.only == Some(block))
                .then(|| Fluid::of(block))
                .flatten();
            if let Some(fluid) = filtered_fluid {
                if !fluid_shell_face_visible(fluid, neighbor, face.neighbor[1]) {
                    continue;
                }
            } else if (block == Block::SnowLayer
                && face_index != FACE_TOP
                && face_index != FACE_BOTTOM
                && neighbor == Some(Block::SnowLayer))
                || neighbor_hides_face_at(
                    block,
                    neighbors,
                    chunk,
                    (x as i32 + face.neighbor[0], ny, z as i32 + face.neighbor[2]),
                    face_index,
                    fancy_graphics,
                )
            {
                // Equal-height snow layers share a side, including across chunks.
                continue;
            }
            let geometry = unit.face(face_index);
            let shading = CornerShading {
                light: face_corner_light(skylight, x, y, z, face, geometry),
                ao: face_corner_ao(chunk, neighbors, x, y, z, face, geometry),
                shade: true,
            };
            let grass_side =
                block == Block::Grass && face_index != FACE_TOP && face_index != FACE_BOTTOM;
            // A side under snow is not a grass side any more: `RenderBlocks`
            // only draws the overlay for tile 3, so the snowy tile gets none.
            let snow_covered = grass_side && snow_above(chunk, neighbors, x, y, z);
            let grass_overlay = grass_side && fancy_graphics && !snow_covered;
            let base = if block == Block::Grass && face_index == FACE_TOP {
                grass_tint
            } else {
                block_tint(block, metadata, foliage)
            };
            let side_tint = if grass_side { [1.0; 3] } else { base };
            let (tile_x, tile_y) = if snow_covered {
                SNOWY_GRASS_SIDE_TILE
            } else {
                block_tile(block, metadata, face_index, fancy_graphics)
            };
            let layer = if filtered_fluid == Some(Fluid::Water) {
                LAYER_WATER
            } else if fancy_graphics && block.is_leaves() {
                LAYER_CUTOUT
            } else if matches!(block, Block::Glass | Block::MobSpawner) {
                LAYER_MASKED
            } else {
                LAYER_OPAQUE
            };
            // Gameplay keeps a corner gradient as its own quad so smooth light
            // stays exact. A block filter is a wireframe, so light and occlusion
            // drop out of the merge key and a long wall becomes one rectangle.
            let recorded = if self.only.is_some() {
                CornerShading {
                    light: [FULL_BRIGHT; 4],
                    ao: [0; 4],
                    shade: shading.shade,
                }
            } else {
                shading
            };
            if self.only.is_some() || uniform_shading(&shading) {
                let (fixed, row, bit) = plane_coords(face_index, x, y, z, band_start);
                let mut base_key = plane_key(layer, [tile_x, tile_y], side_tint, &recorded);
                if block == Block::SnowLayer {
                    base_key.snow_layer = true;
                    // Snow sides can merge horizontally, but never up a full
                    // block: each layer is only an eighth of a block high.
                    if face_index != FACE_TOP && face_index != FACE_BOTTOM {
                        base_key.snow_side_y = Some(y as u8);
                    }
                }
                if grass_overlay {
                    // The coplanar passes must rasterize identical triangles.
                    // Dirt is untinted, but merging it across an overlay tint
                    // boundary gives the two layers different depth rounding.
                    base_key.overlay_tint = Some(quantized_tint(grass_tint));
                }
                insert_plane(
                    planes, face_index, base_key, side_tint, recorded, fixed, row, bit,
                );
                if grass_overlay {
                    insert_plane(
                        planes,
                        face_index,
                        plane_key(LAYER_OVERLAY, GRASS_OVERLAY_TILE, grass_tint, &recorded),
                        grass_tint,
                        recorded,
                        fixed,
                        row,
                        bit,
                    );
                }
            } else {
                let target = layer_mut(meshes, layer);
                target.push_face(
                    origin,
                    face,
                    geometry,
                    face_index,
                    side_tint,
                    shading,
                    block,
                    (tile_x, tile_y),
                );
                if grass_overlay {
                    meshes
                        .grass_overlay
                        .push_grass_overlay(origin, face, face_index, grass_tint, shading);
                }
            }
        }
    }
}
