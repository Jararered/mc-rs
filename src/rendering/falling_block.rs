//! `RenderFallingSand`: a falling block drawn as the world cube it came from.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::entity::PreviousTick;
use crate::entity::falling_block::FallingBlock;
use crate::rendering::meshing::dropped_block_meshes;
use crate::rendering::textures::TerrainMaterial;
use crate::world::tick::WorldTick;

#[derive(Component)]
pub(crate) struct FallingBlockVisual;

/// Give each new falling block the world cube mesh, and slide it between
/// ticks like other entities.
pub(crate) fn sync_falling_block_rendering(
    mut commands: Commands,
    tick: Res<WorldTick>,
    settings: Option<Res<GameSettings>>,
    terrain: Option<Res<TerrainMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: Local<HashMap<(Block, bool), Handle<Mesh>>>,
    falling: Query<(), With<FallingBlock>>,
    new_blocks: Query<(Entity, &FallingBlock), Without<FallingBlockVisual>>,
    mut visuals: Query<
        (&Transform, &PreviousTick, &Children),
        (With<FallingBlock>, With<FallingBlockVisual>),
    >,
    mut pieces: Query<&mut Transform, Without<FallingBlock>>,
) {
    let Some(terrain) = terrain else {
        return;
    };
    let fancy = settings.is_some_and(|settings| settings.graphics.fancy_leaves());
    // A collapse spawns many blocks of one kind: they share a mesh, which is
    // let go once nothing is falling.
    if falling.is_empty() {
        cache.clear();
    }
    for (entity, falling) in &new_blocks {
        let mesh = cache
            .entry((falling.block, fancy))
            .or_insert_with(|| {
                let built = dropped_block_meshes(
                    falling.block,
                    0,
                    fancy,
                    [0.55, 0.8, 0.4],
                    [0.28, 0.71, 0.09],
                );
                meshes.add(built.body.into_mesh())
            })
            .clone();
        let child = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(terrain.0.clone()),
                Transform::default(),
                NoFrustumCulling,
            ))
            .id();
        commands
            .entity(entity)
            .add_child(child)
            .insert(FallingBlockVisual);
    }
    for (transform, previous, children) in &mut visuals {
        let slide = previous.0.lerp(transform.translation, tick.partial()) - transform.translation;
        for child in children.iter() {
            if let Ok(mut piece) = pieces.get_mut(child) {
                piece.translation = slide;
            }
        }
    }
}
