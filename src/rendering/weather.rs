//! Small reusable precipitation meshes follow the player without rewriting assets.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::state::AppScreen;
use crate::player::Player;
use crate::world::biome::Biome;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::tick::WorldTick;
use crate::world::weather::LightningStrike;
use crate::world::weather::WorldWeather;

#[derive(Resource)]
struct WeatherAssets {
    rain: Handle<Mesh>,
    snow: Handle<Mesh>,
    rain_material: Handle<StandardMaterial>,
    snow_material: Handle<StandardMaterial>,
    bolt: Handle<StandardMaterial>,
}

#[derive(Component)]
struct PrecipitationPatch {
    dx: i32,
    dz: i32,
}

#[derive(Component)]
pub(crate) struct LightningFlash(u8);

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, prepare).add_systems(
        Update,
        (update_precipitation, update_lightning).run_if(in_state(AppScreen::Playing)),
    );
}

fn prepare(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let rain = meshes.add(sheet(false));
    let snow = meshes.add(sheet(true));
    let assets = WeatherAssets {
        rain: rain.clone(),
        snow,
        rain_material: materials.add(StandardMaterial {
            base_color: Color::srgba(0.48, 0.66, 0.83, 0.40),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            ..default()
        }),
        snow_material: materials.add(StandardMaterial {
            base_color: Color::srgba(0.95, 0.97, 1.0, 0.78),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            ..default()
        }),
        bolt: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            ..default()
        }),
    };
    for dx in -3..=3 {
        for dz in -3..=3 {
            commands.spawn((
                PrecipitationPatch { dx, dz },
                Mesh3d(rain.clone()),
                MeshMaterial3d(assets.rain_material.clone()),
                Visibility::Hidden,
                Transform::default(),
            ));
        }
    }
    commands.insert_resource(assets);
}

fn sheet(snow: bool) -> Mesh {
    let mut positions = Vec::with_capacity(384);
    let mut normals = Vec::with_capacity(384);
    let mut uvs = Vec::with_capacity(384);
    let mut indices = Vec::with_capacity(576);
    for i in 0..48u32 {
        let x = (i.wrapping_mul(47).wrapping_add(11) % 199) as f32 / 199. * 4. - 2.;
        let z = (i.wrapping_mul(103).wrapping_add(23) % 211) as f32 / 211. * 4. - 2.;
        let y = (i.wrapping_mul(59) % 53) as f32 / 53. * 8. - 4.;
        let (width, length) = if snow { (0.13, 0.13) } else { (0.035, 1.15) };
        for rotate in 0..2 {
            let start = positions.len() as u32;
            let vertices = if rotate == 0 {
                [
                    [x - width, y, z],
                    [x + width, y, z],
                    [x + width, y + length, z],
                    [x - width, y + length, z],
                ]
            } else {
                [
                    [x, y, z - width],
                    [x, y, z + width],
                    [x, y + length, z + width],
                    [x, y + length, z - width],
                ]
            };
            positions.extend(vertices);
            normals.extend([[0., 0., 1.]; 4]);
            uvs.extend([[0., 0.], [1., 0.], [1., 1.], [0., 1.]]);
            indices.extend([start, start + 1, start + 2, start, start + 2, start + 3]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn update_precipitation(
    time: Res<Time>,
    weather: Option<Res<WorldWeather>>,
    chunks: Res<WorldChunks>,
    player: Query<&Transform, (With<Player>, Without<PrecipitationPatch>)>,
    assets: Res<WeatherAssets>,
    mut patches: Query<
        (
            &PrecipitationPatch,
            &mut Transform,
            &mut Visibility,
            &mut Mesh3d,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        Without<Player>,
    >,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let raining = weather.as_ref().is_some_and(|w| w.is_raining());
    let base_x = (player.translation.x.floor() as i32).div_euclid(4) * 4;
    let base_z = (player.translation.z.floor() as i32).div_euclid(4) * 4;
    for (patch, mut transform, mut visibility, mut mesh, mut material) in &mut patches {
        let x = base_x + patch.dx * 4 + 2;
        let z = base_z + patch.dz * 4 + 2;
        let biome = chunks.climate_at(x, z).map(|c| c.biome);
        let snow = matches!(biome, Some(Biome::Taiga | Biome::Tundra | Biome::IceDesert));
        let dry = matches!(biome, None | Some(Biome::Desert));
        let ground = chunks.get(ChunkPosition::from_block(x, z)).map(|chunk| {
            chunk
                .heightmap
                .get(x.rem_euclid(16) as usize, z.rem_euclid(16) as usize)
        });
        *visibility =
            if !raining || dry || ground.is_none_or(|y| y as f32 > player.translation.y + 6.) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        if *visibility == Visibility::Hidden {
            continue;
        }
        let selected_mesh = if snow { &assets.snow } else { &assets.rain };
        let selected_material = if snow {
            &assets.snow_material
        } else {
            &assets.rain_material
        };
        if mesh.0 != *selected_mesh {
            mesh.0 = selected_mesh.clone();
        }
        if material.0 != *selected_material {
            material.0 = selected_material.clone();
        }
        let speed = if snow { 2.5 } else { 10.0 };
        let phase =
            (time.elapsed_secs() * speed + (patch.dx * 31 + patch.dz * 17) as f32).rem_euclid(8.0);
        transform.translation = Vec3::new(x as f32, player.translation.y + 3.0 - phase, z as f32);
    }
}

fn update_lightning(
    mut commands: Commands,
    mut strikes: MessageReader<LightningStrike>,
    mut flashes: Query<(Entity, &mut LightningFlash)>,
    tick: Res<WorldTick>,
    assets: Res<WeatherAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, mut flash) in &mut flashes {
        flash.0 = flash.0.saturating_sub(tick.ticks_this_frame() as u8);
        if flash.0 == 0 {
            commands.entity(entity).despawn();
        }
    }
    for &LightningStrike(pos) in strikes.read() {
        let height = (128.0 - pos.y).max(1.0);
        commands.spawn((
            LightningFlash(5),
            Mesh3d(meshes.add(Cuboid::new(0.16, height, 0.16))),
            MeshMaterial3d(assets.bolt.clone()),
            Transform::from_xyz(pos.x, pos.y + height / 2.0, pos.z),
        ));
    }
}
