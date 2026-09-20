use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::prelude::*;

use crate::app::settings::GameSettings;

mod biome_color;

pub use biome_color::FoliageColors;
pub use biome_color::GrassColors;
pub use biome_color::PALETTE_SIZE;
pub use biome_color::palette_index;

pub struct TerrainTexturePlugin;

impl Plugin for TerrainTexturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameSettings>()
            .add_systems(PreStartup, load_terrain_atlas)
            .add_systems(Update, (apply_terrain_atlas, apply_leaf_alpha_mode));
    }
}

#[derive(Resource)]
pub(crate) struct TerrainMaterial(pub Handle<StandardMaterial>);

#[derive(Resource)]
pub(crate) struct WaterMaterial(pub Handle<StandardMaterial>);

#[derive(Resource)]
struct PendingTerrainAtlas(Handle<Image>);

fn load_terrain_atlas(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    settings: Res<GameSettings>,
) {
    let image = asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::nearest();
        })
        .load("terrain.png");
    let material = materials.add(StandardMaterial {
        perceptual_roughness: 1.0,
        alpha_mode: leaf_alpha_mode(settings.fancy_graphics),
        ..default()
    });
    let water = materials.add(StandardMaterial {
        perceptual_roughness: 1.0,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.insert_resource(TerrainMaterial(material));
    commands.insert_resource(WaterMaterial(water));
    commands.insert_resource(PendingTerrainAtlas(image));
    commands.insert_resource(GrassColors::load());
    commands.insert_resource(FoliageColors::load());
}

fn apply_terrain_atlas(
    mut commands: Commands,
    pending: Option<Res<PendingTerrainAtlas>>,
    images: Res<Assets<Image>>,
    terrain_material: Res<TerrainMaterial>,
    water_material: Res<WaterMaterial>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(pending) = pending else {
        return;
    };
    if images.get(&pending.0).is_none() {
        return;
    }
    if let Some(mut material) = materials.get_mut(&terrain_material.0) {
        material.base_color_texture = Some(pending.0.clone());
    }
    if let Some(mut material) = materials.get_mut(&water_material.0) {
        material.base_color_texture = Some(pending.0.clone());
    }
    commands.remove_resource::<PendingTerrainAtlas>();
}

fn apply_leaf_alpha_mode(
    settings: Res<GameSettings>,
    terrain_material: Res<TerrainMaterial>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !settings.is_changed() {
        return;
    }
    if let Some(mut material) = materials.get_mut(&terrain_material.0) {
        material.alpha_mode = leaf_alpha_mode(settings.fancy_graphics);
    }
}

fn leaf_alpha_mode(fancy_graphics: bool) -> AlphaMode {
    if fancy_graphics {
        // Fancy leaf tiles have punched holes. Mask discards those texels
        // without sorting the whole chunk as transparent.
        AlphaMode::Mask(0.5)
    } else {
        AlphaMode::Opaque
    }
}

// The original terrain.png is a 16 by 16 grid of 16-pixel tiles.
pub(crate) fn block_tile(
    block: super::block::block::BlockId,
    face: usize,
    fancy_graphics: bool,
) -> (u8, u8) {
    use super::block::block::BlockId;

    match block {
        BlockId::Grass if face == 0 => (0, 0),
        BlockId::Grass if face == 1 => (2, 0),
        BlockId::Grass => (3, 0),
        BlockId::Stone => (1, 0),
        BlockId::Dirt => (2, 0),
        BlockId::Cobblestone => (0, 1),
        BlockId::WoodenPlanks => (4, 0),
        BlockId::Bedrock => (1, 1),
        BlockId::Sand => (2, 1),
        BlockId::Gravel => (3, 1),
        BlockId::Wood if face == 0 || face == 1 => (5, 1),
        BlockId::Wood => (4, 1),
        BlockId::SpruceWood if face == 0 || face == 1 => (5, 1),
        BlockId::SpruceWood => (4, 7),
        BlockId::BirchWood if face == 0 || face == 1 => (5, 1),
        BlockId::BirchWood => (5, 7),
        // Fancy leaves use the cutout tile; Fast uses the solid tile one column over.
        BlockId::Leaves | BlockId::BirchLeaves => {
            if fancy_graphics {
                (4, 3)
            } else {
                (5, 3)
            }
        }
        BlockId::SpruceLeaves => {
            if fancy_graphics {
                (4, 8)
            } else {
                (5, 8)
            }
        }
        BlockId::GoldOre => (0, 2),
        BlockId::IronOre => (1, 2),
        BlockId::CoalOre => (2, 2),
        BlockId::Bricks => (7, 0),
        BlockId::MossyCobblestone => (4, 2),
        BlockId::Obsidian => (5, 2),
        BlockId::DiamondOre => (2, 3),
        BlockId::Snow => (2, 4),
        BlockId::Clay => (8, 4),
        BlockId::Netherrack => (7, 6),
        BlockId::Glowstone => (9, 6),
        BlockId::Water => (13, 12),
        BlockId::Ice => (3, 4),
        _ => (1, 0),
    }
}
