use bevy::{
    image::{ImageLoaderSettings, ImageSampler},
    prelude::*,
};

pub struct TerrainTexturePlugin;

impl Plugin for TerrainTexturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_terrain_atlas)
            .add_systems(Update, apply_terrain_atlas);
    }
}

#[derive(Resource)]
pub(crate) struct TerrainMaterial(pub Handle<StandardMaterial>);

#[derive(Resource)]
struct PendingTerrainAtlas(Handle<Image>);

fn load_terrain_atlas(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let image = asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::nearest();
        })
        .load("terrain.png");
    let material = materials.add(StandardMaterial {
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.insert_resource(TerrainMaterial(material));
    commands.insert_resource(PendingTerrainAtlas(image));
}

fn apply_terrain_atlas(
    mut commands: Commands,
    pending: Option<Res<PendingTerrainAtlas>>,
    images: Res<Assets<Image>>,
    terrain_material: Res<TerrainMaterial>,
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
    commands.remove_resource::<PendingTerrainAtlas>();
}

// The original terrain.png is a 16 by 16 grid of 16-pixel tiles.
pub(crate) fn block_tile(block: super::block::block::BlockId, face: usize) -> (u8, u8) {
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
