//! Primed TNT, drawn as a plain red cube. Mobs, arrows, and fireballs use
//! Beta's own models in [`super::creatures`].
use bevy::prelude::*;

use crate::app::state::AppScreen;
use crate::entity::mobs::PrimedTnt;

#[derive(Resource)]
struct TntAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

pub(super) fn plugin(app: &mut App) {
    app.init_asset::<StandardMaterial>()
        .add_systems(Startup, prepare_tnt_assets)
        .add_systems(Update, add_tnt_models.run_if(in_state(AppScreen::Playing)));
}

fn prepare_tnt_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(TntAssets {
        mesh: meshes.add(Cuboid::new(0.98, 0.98, 0.98)),
        material: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(190, 53, 42),
            ..default()
        }),
    });
}

fn add_tnt_models(
    mut commands: Commands,
    assets: Res<TntAssets>,
    tnt: Query<Entity, Added<PrimedTnt>>,
) {
    for entity in &tnt {
        commands.entity(entity).insert((
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.material.clone()),
        ));
    }
}
