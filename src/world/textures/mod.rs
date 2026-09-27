use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::image::ImageSamplerDescriptor;
use bevy::material::OpaqueRendererMethod;
use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::app::settings::GraphicsQuality;
use crate::world::meshing::BlockLighting;
use crate::world::meshing::WATER_ALPHA;
use crate::world::sky::celestial_angle;
use crate::world::sky::skylight_subtracted;
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
pub use water::FlowingWaterTexture;
pub use water::LAVA_FLOW_TILE;
pub use water::LAVA_STILL_TILE;
pub use water::LavaTexture;
pub use water::StillWaterTexture;
pub use water::WATER_FLOW_TILE;
pub use water::WATER_STILL_TILE;
pub use water::write_atlas_tile;

pub struct TerrainTexturePlugin;

impl Plugin for TerrainTexturePlugin {
    fn build(&self, app: &mut App) {
        block_material::plugin(app);
        instance_tint::plugin(app);
        water::render_plugin(app);
        app.init_resource::<GameSettings>()
            .add_systems(PreStartup, load_terrain_atlas)
            .add_systems(
                Update,
                (
                    apply_terrain_atlas,
                    apply_graphics_materials,
                    update_block_lighting.after(apply_graphics_materials),
                    water::animate_fluid_textures.after(apply_terrain_atlas),
                ),
            );
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
        old_lighting: settings.old_lighting,
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

fn block_material(base: StandardMaterial, lighting: BlockLighting, wiggle: f32) -> BlockMaterial {
    BlockMaterial {
        base,
        extension: BlockShading {
            settings: BlockShadingSettings::new(lighting, wiggle),
        },
    }
}

fn load_terrain_atlas(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<BlockMaterial>>,
    settings: Res<GameSettings>,
) {
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
            unlit: settings.old_lighting,
            ..default()
        },
        lighting,
        0.0,
    ));
    let grass_overlay = materials.add(block_material(
        StandardMaterial {
            perceptual_roughness: 1.0,
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: settings.old_lighting,
            ..default()
        },
        lighting,
        0.0,
    ));
    let cutout = materials.add(block_material(
        StandardMaterial {
            perceptual_roughness: 1.0,
            unlit: settings.old_lighting,
            // Fancy leaf tiles have punched holes. Mask discards those texels
            // without sorting the whole chunk as transparent.
            alpha_mode: AlphaMode::Mask(0.5),
            ..default()
        },
        lighting,
        leaf_wiggle_amplitude(&settings),
    ));
    let mut water = StandardMaterial {
        base_color: Color::WHITE.with_alpha(WATER_ALPHA),
        double_sided: true,
        cull_mode: None,
        unlit: settings.old_lighting,
        ..default()
    };
    apply_water_quality(&mut water, settings.graphics);
    let water = materials.add(block_material(water, lighting, 0.0));
    let plants = materials.add(block_material(
        StandardMaterial {
            perceptual_roughness: 1.0,
            alpha_mode: AlphaMode::Mask(0.5),
            cull_mode: None,
            double_sided: true,
            unlit: settings.old_lighting,
            ..default()
        },
        lighting,
        0.0,
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
        material.base.unlit = settings.old_lighting;
        let subtracted = material.extension.settings.lighting().skylight_subtracted;
        let amplitude = if *handle == handles.cutout.0 {
            wiggle
        } else {
            0.0
        };
        material.extension.settings =
            BlockShadingSettings::new(block_lighting(&settings, subtracted), amplitude);
        if *handle == handles.water.0 {
            apply_water_quality(&mut material.base, settings.graphics);
        }
    }
}

/// `World.skylightSubtracted` steps 22 times a day. Each step rewrites the
/// block material uniforms once; no mesh is rebuilt.
fn update_block_lighting(
    tick: Option<Res<WorldTick>>,
    handles: BlockMaterials,
    mut materials: ResMut<Assets<BlockMaterial>>,
) {
    let Some(tick) = tick else {
        return;
    };
    let subtracted = skylight_subtracted(celestial_angle(tick.world_time(), tick.partial()));
    for handle in handles.handles() {
        let unchanged = materials.get(handle).is_none_or(|material| {
            material.extension.settings.lighting().skylight_subtracted == subtracted
        });
        if unchanged {
            continue;
        }
        if let Some(mut material) = materials.get_mut(handle) {
            material.extension.settings.skylight_subtracted = f32::from(subtracted);
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

/// Glossy enough for Bevy screen-space reflections, matching the SSR water demo.
const ULTRA_WATER_ROUGHNESS: f32 = 0.09;

fn apply_water_quality(material: &mut StandardMaterial, graphics: GraphicsQuality) {
    if graphics.realistic_water() {
        // Opaque so the surface writes the deferred G-buffer SSR reads.
        material.perceptual_roughness = ULTRA_WATER_ROUGHNESS;
        material.alpha_mode = AlphaMode::Opaque;
        material.opaque_render_method = OpaqueRendererMethod::Deferred;
        material.reflectance = 0.5;
    } else {
        material.perceptual_roughness = 1.0;
        material.alpha_mode = AlphaMode::Blend;
        material.opaque_render_method = OpaqueRendererMethod::Auto;
        material.reflectance = 0.5;
    }
}

// The original terrain.png is a 16 by 16 grid of 16-pixel tiles.
pub fn block_tile(block: crate::block::id::Id, face: usize, fancy_graphics: bool) -> (u8, u8) {
    use crate::block::id::Id;

    match block {
        Id::Ladder | Id::LadderNorth | Id::LadderEast | Id::LadderSouth | Id::LadderWest => (3, 5),
        Id::Furnace
        | Id::FurnaceNorth
        | Id::FurnaceEast
        | Id::FurnaceSouth
        | Id::FurnaceWest
        | Id::LitFurnace
        | Id::LitFurnaceNorth
        | Id::LitFurnaceEast
        | Id::LitFurnaceSouth
        | Id::LitFurnaceWest
            if face == 0 || face == 1 =>
        {
            (14, 3)
        }
        Id::Furnace
        | Id::FurnaceNorth
        | Id::FurnaceEast
        | Id::FurnaceSouth
        | Id::FurnaceWest
        | Id::LitFurnace
        | Id::LitFurnaceNorth
        | Id::LitFurnaceEast
        | Id::LitFurnaceSouth
        | Id::LitFurnaceWest => {
            let facing = block.furnace_facing().expect("matched furnace");
            if face == facing.face_index() {
                if block.is_lit_furnace() {
                    (13, 3)
                } else {
                    (12, 2)
                }
            } else {
                (13, 2)
            }
        }
        Id::Grass if face == 0 => (0, 0),
        Id::Grass if face == 1 => (2, 0),
        Id::Grass => (3, 0),
        Id::Stone => (1, 0),
        Id::Dirt => (2, 0),
        Id::Farmland if face == 0 => farmland_top_tile(false),
        Id::Farmland => (2, 0),
        Id::Cobblestone => (0, 1),
        Id::WoodenPlanks | Id::SprucePlanks | Id::BirchPlanks => (4, 0),
        // Beta BlockWorkbench: top 43, plank bottom 4, and two alternating
        // side tiles (59/60) based on the block face orientation.
        Id::CraftingTable if face == 0 => (11, 2),
        Id::CraftingTable if face == 1 => (4, 0),
        Id::CraftingTable if face == 2 || face == 4 => (12, 3),
        Id::CraftingTable => (11, 3),
        Id::Pumpkin | Id::PumpkinNorth | Id::PumpkinEast | Id::PumpkinSouth | Id::PumpkinWest
            if face == 0 || face == 1 =>
        {
            (6, 6)
        }
        Id::Pumpkin | Id::PumpkinNorth | Id::PumpkinEast | Id::PumpkinSouth | Id::PumpkinWest => {
            if block
                .pumpkin_facing()
                .is_some_and(|facing| facing.face_index() == face)
            {
                (7, 7)
            } else {
                (6, 7)
            }
        }
        Id::JackOLantern if face == 0 || face == 1 => (6, 6),
        Id::JackOLantern if face == 3 => (8, 7),
        Id::JackOLantern => (6, 7),
        Id::Bedrock => (1, 1),
        Id::Sand => (2, 1),
        Id::Gravel => (3, 1),
        Id::Wood if face == 0 || face == 1 => (5, 1),
        Id::Wood => (4, 1),
        Id::SpruceWood if face == 0 || face == 1 => (5, 1),
        Id::SpruceWood => (4, 7),
        Id::BirchWood if face == 0 || face == 1 => (5, 1),
        Id::BirchWood => (5, 7),
        // Fancy leaves use the cutout tile; Fast uses the solid tile one column over.
        Id::Leaves | Id::BirchLeaves => {
            if fancy_graphics {
                (4, 3)
            } else {
                (5, 3)
            }
        }
        Id::SpruceLeaves => {
            if fancy_graphics {
                (4, 8)
            } else {
                (5, 8)
            }
        }
        Id::Sponge => (0, 3),
        Id::GoldBlock => (7, 1),
        Id::IronBlock => (6, 1),
        Id::DiamondBlock => (8, 1),
        Id::Bookshelf if face == 0 || face == 1 => (4, 0),
        Id::Bookshelf => (3, 2),
        Id::Tnt if face == 0 => (9, 0),
        Id::Tnt if face == 1 => (10, 0),
        Id::Tnt => (8, 0),
        Id::Sandstone if face == 0 => (0, 11),
        Id::Sandstone if face == 1 => (0, 13),
        Id::Sandstone => (0, 12),
        Id::LapisOre => (0, 10),
        Id::LapisBlock => (0, 9),
        Id::RedstoneOre | Id::LitRedstoneOre => (3, 3),
        Id::RedstoneWire => (4, 10),
        Id::Repeater | Id::PoweredRepeater => match face {
            0 => (3, if block == Id::Repeater { 8 } else { 9 }),
            1 => (3, if block == Id::Repeater { 7 } else { 6 }),
            _ => (5, 0),
        },
        Id::RedstoneTorch => (3, 6),
        Id::UnlitRedstoneTorch => (3, 7),
        Id::StoneButton | Id::StonePressurePlate => (1, 0),
        Id::WoodenPressurePlate => (4, 0),
        Id::Lever => (0, 6),
        Id::WoodenDoor => (1, 6),
        Id::IronDoor => (2, 6),
        Id::Trapdoor => (4, 5),
        Id::Rail => (0, 8),
        Id::PoweredRail => (3, 11),
        Id::DetectorRail => (3, 12),
        Id::Piston | Id::StickyPiston if face == 0 => {
            (if block == Id::StickyPiston { 10 } else { 11 }, 6)
        }
        Id::Piston | Id::StickyPiston if face == 1 => (13, 6),
        Id::Piston | Id::StickyPiston => (12, 6),
        Id::Dispenser if face == 0 || face == 1 => (14, 3),
        Id::Dispenser if face == 4 => (14, 2),
        Id::Dispenser => (13, 2),
        Id::GoldOre => (0, 2),
        Id::IronOre => (1, 2),
        Id::CoalOre => (2, 2),
        Id::Bricks => (7, 0),
        Id::MossyCobblestone => (4, 2),
        Id::Obsidian => (5, 2),
        Id::DiamondOre => (2, 3),
        Id::SnowLayer | Id::Snow => (2, 4),
        Id::Cactus if face == 0 => (5, 4),
        Id::Cactus if face == 1 => (7, 4),
        Id::Cactus => (6, 4),
        Id::SugarCane => (9, 4),
        Id::Clay => (8, 4),
        Id::MobSpawner => (1, 4),
        Id::ChestNorth | Id::ChestEast | Id::ChestSouth | Id::ChestWest | Id::Chest
            if face == 0 || face == 1 =>
        {
            (9, 1)
        }
        Id::ChestNorth | Id::ChestEast | Id::ChestSouth | Id::ChestWest | Id::Chest => {
            if block
                .chest_facing()
                .is_some_and(|facing| facing.face_index() == face)
            {
                (11, 1)
            } else {
                (10, 1)
            }
        }
        Id::Lava | Id::FlowingLava => (13, 14),
        Id::Netherrack => (7, 6),
        Id::Glowstone => (9, 6),
        Id::Torch | Id::TorchWest | Id::TorchEast | Id::TorchNorth | Id::TorchSouth => (0, 5),
        Id::Dandelion => (13, 0),
        Id::Rose => (12, 0),
        Id::DeadBush => (7, 3),
        Id::RedMushroom => (12, 1),
        Id::BrownMushroom => (13, 1),
        Id::TallGrass => (7, 2),
        Id::Fern => (8, 3),
        Id::Water | Id::FlowingWater => water::WATER_STILL_TILE,
        Id::Crops => crop_tile(7),
        Id::Ice => (3, 4),
        _ => (1, 0),
    }
}

/// Atlas tile for a crop at growth `stage` (`BlockCrops`: 88 + stage).
pub fn crop_tile(stage: u8) -> (u8, u8) {
    (8 + stage.min(7), 5)
}

/// Atlas tile for Beta farmland's dry or hydrated top face.
pub fn farmland_top_tile(wet: bool) -> (u8, u8) {
    if wet { (6, 5) } else { (7, 5) }
}
