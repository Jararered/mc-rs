use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::image::ImageSamplerDescriptor;
use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy::render::storage::ShaderBuffer;

use crate::app::settings::GameSettings;
use crate::block::blocks::species;
use crate::rendering::chunk_quads::ChunkQuads;
use crate::rendering::meshing::BlockLighting;
use crate::rendering::meshing::WATER_ALPHA;
use crate::world::tick::WorldTick;

/// `terrain.png` is a 16×16 grid of square tiles.
pub const ATLAS_GRID: u32 = 16;
/// Beta tile size in pixels. HD packs use a power-of-two multiple of this.
pub const ATLAS_TILE_PX: u32 = 16;
/// Extra texels duplicated around each tile so MSAA cannot sample a neighbour.
pub const ATLAS_PAD_TEXELS: u32 = 2;

mod biome_color;
mod block_material;
mod instance_tint;
mod water;
mod wireframe;

pub use biome_color::FoliageColors;
pub use biome_color::GrassColors;
pub use biome_color::PALETTE_SIZE;
pub use biome_color::palette_index;
pub use block_material::BlockMaterial;
pub use block_material::BlockShading;
pub use block_material::BlockShadingSettings;
pub use block_material::LEAF_WIGGLE_AMPLITUDE;
pub use instance_tint::InstanceTint;
pub use instance_tint::TintedMaterial;
pub use instance_tint::tint_tag;
pub use water::FIRE_TILE;
pub use water::FIRE_TILE_ALT;
pub use water::FlamesTexture;
pub use water::FlowingWaterTexture;
pub use water::LAVA_FLOW_TILE;
pub use water::LAVA_STILL_TILE;
pub use water::LavaTexture;
pub use water::PORTAL_TILE;
pub use water::PortalTexture;
pub use water::StillWaterTexture;
pub use water::WATER_FLOW_TILE;
pub use water::WATER_STILL_TILE;
pub use water::write_atlas_tile;
pub(crate) use wireframe::LineRasterSupported;
pub use wireframe::MeshWireframe;
pub use wireframe::MeshWireframePlugin;
pub use wireframe::configure_mesh_wireframe;

pub struct TerrainTexturePlugin;

impl Plugin for TerrainTexturePlugin {
    fn build(&self, app: &mut App) {
        block_material::plugin(app);
        crate::rendering::chunk_quads::plugin(app);
        instance_tint::plugin(app);
        water::render_plugin(app);
        app.init_resource::<GameSettings>()
            .add_plugins(MeshWireframePlugin)
            .add_systems(PreStartup, load_terrain_atlas)
            .add_systems(
                Update,
                (
                    apply_terrain_atlas,
                    apply_graphics_materials,
                    update_block_lighting.after(apply_graphics_materials),
                    water::animate_fluid_textures.after(apply_terrain_atlas),
                ),
            )
            // After streaming asked for room, and before this frame's asset
            // changes are sent to the renderer.
            .add_systems(
                PostUpdate,
                sync_quad_buffer.before(bevy::asset::AssetEventSystems),
            );
    }

    fn finish(&self, app: &mut App) {
        wireframe::detect_line_raster(app);
    }
}

#[derive(Resource)]
pub(crate) struct TerrainMaterial(pub Handle<BlockMaterial>);

#[derive(Resource)]
pub(crate) struct GrassOverlayMaterial(pub Handle<BlockMaterial>);

/// Fancy leaves: alpha-masked and wiggling.
#[derive(Resource)]
pub(crate) struct CutoutMaterial(pub Handle<BlockMaterial>);

#[derive(Resource)]
pub(crate) struct WaterMaterial(pub Handle<BlockMaterial>);

/// Static alpha-masked geometry such as crossed plants and cactus blocks.
#[derive(Resource)]
pub(crate) struct AlphaMaskMaterial(pub Handle<BlockMaterial>);

#[derive(Resource)]
struct PendingTerrainAtlas(Handle<Image>);

fn nearest_atlas_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        lod_max_clamp: 0.0,
        ..ImageSamplerDescriptor::nearest()
    })
}

fn block_lighting(settings: &GameSettings, skylight_subtracted: u8) -> BlockLighting {
    BlockLighting {
        smooth_lighting: settings.smooth_lighting,
        skylight_subtracted,
    }
}

fn leaf_wiggle_amplitude(settings: &GameSettings) -> f32 {
    if settings.wiggle_leaves {
        LEAF_WIGGLE_AMPLITUDE
    } else {
        0.0
    }
}

fn block_material(
    base: StandardMaterial,
    lighting: BlockLighting,
    wiggle: f32,
    quads: &Handle<ShaderBuffer>,
) -> BlockMaterial {
    BlockMaterial {
        base,
        extension: BlockShading {
            settings: BlockShadingSettings::new(lighting, wiggle),
            quads: quads.clone(),
            wireframe: false,
        },
    }
}

fn load_terrain_atlas(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<BlockMaterial>>,
    settings: Res<GameSettings>,
    quads: Option<Res<ChunkQuads>>,
) {
    // Without a renderer there is no quad buffer and nothing draws.
    let quads = &quads.map_or_else(Handle::default, |quads| quads.buffer());
    let image = asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = nearest_atlas_sampler();
        })
        .load("terrain.png");
    let lighting = block_lighting(&settings, 0);
    let material = materials.add(block_material(
        StandardMaterial {
            perceptual_roughness: 1.0,
            alpha_mode: AlphaMode::Opaque,
            unlit: true,
            ..default()
        },
        lighting,
        0.0,
        quads,
    ));
    let grass_overlay = materials.add(block_material(
        StandardMaterial {
            perceptual_roughness: 1.0,
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            ..default()
        },
        lighting,
        0.0,
        quads,
    ));
    let cutout = materials.add(block_material(
        StandardMaterial {
            perceptual_roughness: 1.0,
            unlit: true,
            // Fancy leaf tiles have punched holes. Mask discards those texels
            // without sorting the whole chunk as transparent.
            alpha_mode: AlphaMode::Mask(0.5),
            ..default()
        },
        lighting,
        leaf_wiggle_amplitude(&settings),
        quads,
    ));
    let water = StandardMaterial {
        base_color: Color::WHITE.with_alpha(WATER_ALPHA),
        double_sided: true,
        cull_mode: None,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    };
    let water = materials.add(block_material(water, lighting, 0.0, quads));
    let plants = materials.add(block_material(
        StandardMaterial {
            perceptual_roughness: 1.0,
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            ..default()
        },
        lighting,
        0.0,
        quads,
    ));
    commands.insert_resource(TerrainMaterial(material));
    commands.insert_resource(GrassOverlayMaterial(grass_overlay));
    commands.insert_resource(CutoutMaterial(cutout));
    commands.insert_resource(WaterMaterial(water));
    commands.insert_resource(AlphaMaskMaterial(plants));
    commands.insert_resource(PendingTerrainAtlas(image));
    commands.insert_resource(GrassColors::load());
    commands.insert_resource(FoliageColors::load());
}

/// The five block material handles, in the layer order meshes use.
#[derive(bevy::ecs::system::SystemParam)]
struct BlockMaterials<'w> {
    terrain: Res<'w, TerrainMaterial>,
    grass_overlay: Res<'w, GrassOverlayMaterial>,
    cutout: Res<'w, CutoutMaterial>,
    water: Res<'w, WaterMaterial>,
    alpha_mask: Res<'w, AlphaMaskMaterial>,
}

impl BlockMaterials<'_> {
    fn handles(&self) -> [&Handle<BlockMaterial>; 5] {
        [
            &self.terrain.0,
            &self.grass_overlay.0,
            &self.cutout.0,
            &self.water.0,
            &self.alpha_mask.0,
        ]
    }
}

fn apply_terrain_atlas(
    mut commands: Commands,
    pending: Option<Res<PendingTerrainAtlas>>,
    mut images: ResMut<Assets<Image>>,
    handles: BlockMaterials,
    mut materials: ResMut<Assets<BlockMaterial>>,
) {
    let Some(pending) = pending else {
        return;
    };
    let Some(mut image) = images.get_mut(&pending.0) else {
        return;
    };
    pad_atlas_tiles(&mut image);
    image.sampler = nearest_atlas_sampler();
    let handle = pending.0.clone();
    for material in handles.handles() {
        if let Some(mut material) = materials.get_mut(material) {
            material.base.base_color_texture = Some(handle.clone());
        }
    }
    water::start_fluid_animation(&mut commands, &handle, &mut image);
    commands.remove_resource::<PendingTerrainAtlas>();
}

fn apply_graphics_materials(
    settings: Res<GameSettings>,
    handles: BlockMaterials,
    mut materials: ResMut<Assets<BlockMaterial>>,
) {
    if !settings.is_changed() {
        return;
    }
    let wiggle = leaf_wiggle_amplitude(&settings);
    for handle in handles.handles() {
        let Some(mut material) = materials.get_mut(handle) else {
            continue;
        };
        let subtracted = material.extension.settings.lighting().skylight_subtracted;
        let amplitude = if *handle == handles.cutout.0 {
            wiggle
        } else {
            0.0
        };
        material.extension.settings =
            BlockShadingSettings::new(block_lighting(&settings, subtracted), amplitude);
    }
}

/// Grow the quad buffer to what chunk layers asked for. A replaced buffer is
/// a new bind group entry, so the materials are marked changed to pick it up.
fn sync_quad_buffer(
    quads: Option<ResMut<ChunkQuads>>,
    buffers: Option<ResMut<Assets<ShaderBuffer>>>,
    device: Option<Res<RenderDevice>>,
    handles: BlockMaterials,
    mut materials: ResMut<Assets<BlockMaterial>>,
) {
    let (Some(mut quads), Some(mut buffers), Some(device)) = (quads, buffers, device) else {
        return;
    };
    if quads.sync(&mut buffers, &device) {
        for handle in handles.handles() {
            // Writing through the guard is what marks the material changed.
            if let Some(mut material) = materials.get_mut(handle) {
                material.extension.quads = quads.buffer();
            }
        }
    }
}

/// `World.skylightSubtracted` steps 22 times a day. Each step rewrites the
/// block material uniforms once; no mesh is rebuilt.
fn update_block_lighting(
    tick: Option<Res<WorldTick>>,
    environment: crate::world::dimension::Environment,
    handles: BlockMaterials,
    mut materials: ResMut<Assets<BlockMaterial>>,
) {
    let Some(tick) = tick else {
        return;
    };
    let subtracted = environment.skylight_subtracted(tick.partial());
    let ambient = environment.ambient_light();
    for handle in handles.handles() {
        let unchanged = materials.get(handle).is_none_or(|material| {
            let settings = &material.extension.settings;
            settings.lighting().skylight_subtracted == subtracted && settings.ambient == ambient
        });
        if unchanged {
            continue;
        }
        if let Some(mut material) = materials.get_mut(handle) {
            material.extension.settings.skylight_subtracted = f32::from(subtracted);
            material.extension.settings.ambient = ambient;
        }
    }
}

/// UV min/max of one atlas tile, inset to the inner (unpadded) 16×16 texels.
pub fn atlas_tile_uvs(tile_x: u8, tile_y: u8) -> (f32, f32, f32, f32) {
    let stride = (ATLAS_TILE_PX + 2 * ATLAS_PAD_TEXELS) as f32;
    let atlas = ATLAS_GRID as f32 * stride;
    let pad = ATLAS_PAD_TEXELS as f32;
    let tile = ATLAS_TILE_PX as f32;
    let u0 = (f32::from(tile_x) * stride + pad) / atlas;
    let v0 = (f32::from(tile_y) * stride + pad) / atlas;
    let u1 = (f32::from(tile_x) * stride + pad + tile) / atlas;
    let v1 = (f32::from(tile_y) * stride + pad + tile) / atlas;
    (u0, v0, u1, v1)
}

/// Duplicate each tile's edge texels into a gutter so filtering and MSAA at a
/// block edge cannot pick up the neighbouring atlas tile.
pub fn pad_atlas_tiles(image: &mut Image) {
    let width = image.texture_descriptor.size.width;
    let height = image.texture_descriptor.size.height;
    if width != height || width % ATLAS_GRID != 0 {
        return;
    }
    let tile = width / ATLAS_GRID;
    if tile < ATLAS_TILE_PX || !tile.is_power_of_two() {
        return;
    }
    let pad = ATLAS_PAD_TEXELS * tile / ATLAS_TILE_PX;
    let stride = tile + 2 * pad;
    let padded = ATLAS_GRID * stride;
    let Some(src) = image.data.as_ref() else {
        return;
    };
    let bpp = 4usize;
    if src.len() != width as usize * height as usize * bpp {
        return;
    }

    let mut dst = vec![0u8; padded as usize * padded as usize * bpp];
    for ty in 0..ATLAS_GRID {
        for tx in 0..ATLAS_GRID {
            for py in 0..stride {
                for px in 0..stride {
                    let sx = (px as i32 - pad as i32).clamp(0, tile as i32 - 1) as u32;
                    let sy = (py as i32 - pad as i32).clamp(0, tile as i32 - 1) as u32;
                    let src_index = ((ty * tile + sy) * width + tx * tile + sx) as usize * bpp;
                    let dst_index = ((ty * stride + py) * padded + tx * stride + px) as usize * bpp;
                    dst[dst_index..dst_index + bpp]
                        .copy_from_slice(&src[src_index..src_index + bpp]);
                }
            }
        }
    }

    image.texture_descriptor.size.width = padded;
    image.texture_descriptor.size.height = padded;
    image.data = Some(dst);
}

// The original terrain.png is a 16 by 16 grid of 16-pixel tiles.
///
/// `metadata` picks the species of wood and leaves and the facing of furnaces,
/// chests, and pumpkins.
pub fn block_tile(
    block: crate::block::blocks::Block,
    metadata: u8,
    face: usize,
    fancy_graphics: bool,
) -> (u8, u8) {
    use crate::block::blocks::Block;

    match block {
        Block::Ladder => (3, 5),
        Block::Furnace | Block::LitFurnace if face == 0 || face == 1 => (14, 3),
        Block::Furnace | Block::LitFurnace => {
            if block
                .facing(metadata)
                .is_some_and(|facing| facing.face_index() == face)
            {
                if block == Block::LitFurnace {
                    (13, 3)
                } else {
                    (12, 2)
                }
            } else {
                (13, 2)
            }
        }
        Block::Grass if face == 0 => (0, 0),
        Block::Grass if face == 1 => (2, 0),
        Block::Grass => (3, 0),
        Block::Stone => (1, 0),
        Block::Dirt => (2, 0),
        Block::Farmland if face == 0 => farmland_top_tile(false),
        Block::Farmland => (2, 0),
        Block::Cobblestone => (0, 1),
        Block::WoodenPlanks => (4, 0),
        // Beta BlockWorkbench: top 43, plank bottom 4, and two alternating
        // side tiles (59/60) based on the block face orientation.
        Block::CraftingTable if face == 0 => (11, 2),
        Block::CraftingTable if face == 1 => (4, 0),
        Block::CraftingTable if face == 2 || face == 4 => (12, 3),
        Block::CraftingTable => (11, 3),
        Block::Pumpkin if face == 0 || face == 1 => (6, 6),
        Block::Pumpkin => {
            if block
                .facing(metadata)
                .is_some_and(|facing| facing.face_index() == face)
            {
                (7, 7)
            } else {
                (6, 7)
            }
        }
        Block::JackOLantern if face == 0 || face == 1 => (6, 6),
        Block::JackOLantern if face == 3 => (8, 7),
        Block::JackOLantern => (6, 7),
        Block::Bedrock => (1, 1),
        Block::Bed => bed_tile(metadata, face),
        Block::Sand => (2, 1),
        Block::Gravel => (3, 1),
        Block::Wood if face == 0 || face == 1 => (5, 1),
        Block::Wood => match metadata & 3 {
            species::SPRUCE => (4, 7),
            species::BIRCH => (5, 7),
            _ => (4, 1),
        },
        // Fancy leaves use the cutout tile; Fast uses the solid tile one column over.
        Block::Leaves => {
            let spruce = metadata & 3 == species::SPRUCE;
            match (spruce, fancy_graphics) {
                (false, true) => (4, 3),
                (false, false) => (5, 3),
                (true, true) => (4, 8),
                (true, false) => (5, 8),
            }
        }
        Block::Sponge => (0, 3),
        Block::GoldBlock => (7, 1),
        Block::IronBlock => (6, 1),
        Block::DiamondBlock => (8, 1),
        Block::Bookshelf if face == 0 || face == 1 => (4, 0),
        Block::Bookshelf => (3, 2),
        Block::Tnt if face == 0 => (9, 0),
        Block::Tnt if face == 1 => (10, 0),
        Block::Tnt => (8, 0),
        Block::Sandstone if face == 0 => (0, 11),
        Block::Sandstone if face == 1 => (0, 13),
        Block::Sandstone => (0, 12),
        Block::LapisOre => (0, 10),
        Block::LapisBlock => (0, 9),
        Block::RedstoneOre | Block::LitRedstoneOre => (3, 3),
        Block::GoldOre => (0, 2),
        Block::IronOre => (1, 2),
        Block::CoalOre => (2, 2),
        Block::Bricks => (7, 0),
        Block::MossyCobblestone => (4, 2),
        Block::Obsidian => (5, 2),
        Block::DiamondOre => (2, 3),
        Block::SnowLayer | Block::Snow => (2, 4),
        Block::Cactus if face == 0 => (5, 4),
        Block::Cactus if face == 1 => (7, 4),
        Block::Cactus => (6, 4),
        Block::SugarCane => (9, 4),
        Block::Clay => (8, 4),
        Block::MobSpawner => (1, 4),
        Block::Fire => water::FIRE_TILE,
        Block::Chest if face == 0 || face == 1 => (9, 1),
        Block::Chest => {
            if block
                .facing(metadata)
                .is_some_and(|facing| facing.face_index() == face)
            {
                (11, 1)
            } else {
                (10, 1)
            }
        }
        Block::Lava | Block::FlowingLava => (13, 14),
        Block::Netherrack => (7, 6),
        Block::NetherPortal => water::PORTAL_TILE,
        Block::Glowstone => (9, 6),
        Block::Torch => (0, 5),
        Block::Dandelion => (13, 0),
        Block::Rose => (12, 0),
        Block::DeadBush => (7, 3),
        Block::RedMushroom => (12, 1),
        Block::BrownMushroom => (13, 1),
        Block::TallGrass if metadata & 3 == species::FERN => (8, 3),
        Block::TallGrass => (7, 2),
        Block::Water | Block::FlowingWater => water::WATER_STILL_TILE,
        Block::Crops => crop_tile(7),
        Block::Ice => (3, 4),
        // `BlockSapling.getBlockTextureFromSideAndMetadata`.
        Block::Sapling => match metadata & 3 {
            species::SPRUCE => tile(63),
            species::BIRCH => tile(79),
            _ => tile(15),
        },
        Block::Glass => tile(49),
        Block::Wool => tile(wool_tile(metadata)),
        // `BlockStep`: stone, sandstone, wooden, and cobblestone slabs.
        Block::StoneSlab | Block::DoubleStoneSlab => match (metadata, face) {
            (0, 0 | 1) => tile(6),
            (0, _) => tile(5),
            (1, 0) => tile(176),
            (1, 1) => tile(208),
            (1, _) => tile(192),
            (2, _) => tile(4),
            (3, _) => tile(16),
            _ => tile(6),
        },
        Block::WoodenStairs | Block::Fence => tile(4),
        Block::CobblestoneStairs => tile(16),
        Block::Cobweb => tile(11),
        Block::Trapdoor => tile(84),
        Block::SoulSand => tile(104),
        Block::NoteBlock => tile(74),
        Block::Jukebox if face == 0 => tile(75),
        Block::Jukebox => tile(74),
        Block::Dispenser if face == 0 || face == 1 => tile(62),
        Block::Dispenser => {
            if block
                .facing(metadata)
                .is_some_and(|facing| facing.face_index() == face)
            {
                tile(46)
            } else {
                tile(45)
            }
        }
        // `BlockLockedChest`: a chest whose front always faces south.
        Block::LockedChest if face == 0 || face == 1 => tile(25),
        Block::LockedChest if face == 4 => tile(27),
        Block::LockedChest => tile(26),
        Block::WoodenDoor | Block::IronDoor => door_tile(block, metadata, face).0,
        Block::RedstoneWire => (4, 10),
        // `BlockRedstoneRepeater`: the top and torch shade with power.
        Block::Repeater if face == 0 => tile(131),
        Block::PoweredRepeater if face == 0 => tile(147),
        Block::Repeater | Block::PoweredRepeater => tile(5),
        Block::RedstoneTorch => tile(99),
        Block::UnlitRedstoneTorch => tile(115),
        Block::StoneButton | Block::StonePressurePlate => (1, 0),
        Block::WoodenPressurePlate => (4, 0),
        Block::Lever => (0, 6),
        // `BlockRail.getBlockTextureFromSideAndMetadata`: curves use the tile
        // above, and powered rails dim when unpowered.
        Block::Rail if metadata >= 6 => tile(112),
        Block::Rail => tile(128),
        Block::PoweredRail if metadata & 8 == 0 => tile(163),
        Block::PoweredRail => tile(179),
        Block::DetectorRail => tile(195),
        Block::Piston | Block::StickyPiston if face == 0 => tile(if block == Block::StickyPiston {
            106
        } else {
            107
        }),
        Block::Piston | Block::StickyPiston if face == 1 => tile(109),
        Block::Piston | Block::StickyPiston | Block::PistonHead => tile(108),
        _ => (1, 0),
    }
}

/// A `terrain.png` tile index as its column and row.
/// `BlockBed.getBlockTextureFromSideAndMetadata`.
fn bed_tile(metadata: u8, face: usize) -> (u8, u8) {
    use crate::block::bed;

    /// `FACES` indices (top, bottom, east, west, south, north) as Java sides.
    const JAVA_SIDE: [usize; 6] = [1, 0, 5, 4, 3, 2];
    /// `ModelBed.bedDirection`.
    const BED_DIRECTION: [[u8; 6]; 4] = [
        [1, 0, 3, 2, 5, 4],
        [1, 0, 5, 4, 2, 3],
        [1, 0, 2, 3, 4, 5],
        [1, 0, 4, 5, 3, 2],
    ];
    let side = JAVA_SIDE[face.min(5)];
    if side == 0 {
        return (4, 0);
    }
    let turned = BED_DIRECTION[bed::direction(metadata)][side];
    tile(match (bed::is_foot(metadata), turned) {
        (true, 2) => 152,
        (true, 4 | 5) => 151,
        (true, _) => 135,
        (false, 3) => 149,
        (false, 4 | 5) => 150,
        (false, _) => 134,
    })
}

const fn tile(index: u8) -> (u8, u8) {
    (index % 16, index / 16)
}

/// `BlockCloth.getBlockTextureFromSideAndMetadata`: white is tile 64 and the
/// dyed colors fill two columns beside it.
pub const fn wool_tile(metadata: u8) -> u8 {
    let metadata = metadata & 15;
    if metadata == 0 {
        return 64;
    }
    let inverted = !metadata & 15;
    113 + ((inverted & 8) >> 3) + (inverted & 7) * 16
}

/// `BlockDoor.getBlockTextureFromSideAndMetadata`: the tile for a door half
/// and whether `RenderBlocks.renderBlockDoor` mirrors it. The upper half
/// (`metadata & 8`) is one row above the lower in the atlas. `face` uses the
/// mesher's order; Beta's side 2 is north and 3 south, 4 west and 5 east.
pub fn door_tile(
    block: crate::block::blocks::Block,
    metadata: u8,
    face: usize,
) -> ((u8, u8), bool) {
    let base: i32 = if block == crate::block::blocks::Block::IronDoor {
        98
    } else {
        97
    };
    let side: i32 = match face {
        0 => 1,
        1 => 0,
        2 => 5,
        3 => 4,
        4 => 3,
        _ => 2,
    };
    let metadata = i32::from(metadata);
    if side == 0 || side == 1 {
        return (tile(base as u8), false);
    }
    let state = if metadata & 4 == 0 {
        (metadata - 1) & 3
    } else {
        metadata & 3
    };
    if (state == 0 || state == 2) ^ (side <= 3) {
        return (tile(base as u8), false);
    }
    let mut turn = state / 2 + ((side & 1) ^ state);
    turn += (metadata & 4) / 4;
    let index = base - (metadata & 8) * 2;
    (tile(index as u8), turn & 1 != 0)
}

/// Atlas tile for a crop at growth `stage` (`BlockCrops`: 88 + stage).
pub fn crop_tile(stage: u8) -> (u8, u8) {
    (8 + stage.min(7), 5)
}

/// Atlas tile for Beta farmland's dry or hydrated top face.
pub fn farmland_top_tile(wet: bool) -> (u8, u8) {
    if wet { (6, 5) } else { (7, 5) }
}

/// Atlas tile for a grass block's side while snow sits on top of it. Beta's
/// `BlockGrass.getBlockTexture` returns tile 68 when the block above carries
/// `Material.snow` or `Material.builtSnow`, in place of the usual side tile 3.
pub const SNOWY_GRASS_SIDE_TILE: (u8, u8) = (4, 4);
