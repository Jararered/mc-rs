use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use game::app::state::AppScreen;
use game::entity::particles::BlockParticlePlugin;
use game::entity::particles::BlockParticles;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::world::block::block::BlockId;
use game::world::chunk::WorldChunks;
use game::world::textures::atlas_tile_uvs;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        StatesPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
    )))
    .init_state::<AppScreen>()
    .init_resource::<WorldChunks>()
    .add_plugins(BlockParticlePlugin);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app
}

fn stone_hit() -> BlockHit {
    BlockHit {
        x: 1,
        y: 64,
        z: 2,
        face: BlockFace::Up,
        block: BlockId::Stone,
    }
}

#[test]
fn block_break_spawns_sixty_four_textured_billboards_and_expires() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<BlockParticles>()
        .emit_break(stone_hit());
    app.update();

    assert_eq!(app.world().resource::<BlockParticles>().active_count(), 64);
    let mut renderers = app.world_mut().query::<(&Name, &Mesh3d)>();
    let (_, handle) = renderers
        .iter(app.world())
        .find(|(name, _)| name.as_str() == "Block particles")
        .unwrap();
    let mesh = app
        .world()
        .resource::<Assets<Mesh>>()
        .get(&handle.0)
        .unwrap();
    assert_eq!(mesh.count_vertices(), 64 * 4);
    let uvs = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap();
    let bevy::mesh::VertexAttributeValues::Float32x2(uvs) = uvs else {
        panic!("particle UVs should be float pairs");
    };
    let (u0, v0, u1, v1) = atlas_tile_uvs(1, 0);
    assert!(
        uvs.iter()
            .all(|uv| { (u0..=u1).contains(&uv[0]) && (v0..=v1).contains(&uv[1]) }),
        "stone debris should sample only the stone atlas tile"
    );

    for _ in 0..41 {
        app.update();
    }
    assert_eq!(app.world().resource::<BlockParticles>().active_count(), 0);
}

#[test]
fn mining_chip_is_small_and_particle_pool_is_bounded() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<BlockParticles>()
        .emit_hit(stone_hit());
    app.update();
    assert_eq!(app.world().resource::<BlockParticles>().active_count(), 1);

    for _ in 0..65 {
        app.world_mut()
            .resource_mut::<BlockParticles>()
            .emit_break(stone_hit());
    }
    app.update();
    assert_eq!(
        app.world().resource::<BlockParticles>().active_count(),
        4_000
    );
}

#[test]
fn debris_moves_on_render_frames_between_simulation_ticks() {
    let mut app = test_app();
    *app.world_mut().resource_mut::<TimeUpdateStrategy>() =
        TimeUpdateStrategy::ManualDuration(Duration::from_millis(10));
    app.world_mut()
        .resource_mut::<BlockParticles>()
        .emit_break(stone_hit());

    let mut positions = Vec::new();
    for _ in 0..15 {
        app.update();
        let mut renderers = app.world_mut().query::<(&Name, &Mesh3d)>();
        let (_, handle) = renderers
            .iter(app.world())
            .find(|(name, _)| name.as_str() == "Block particles")
            .unwrap();
        let mesh = app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&handle.0)
            .unwrap();
        let bevy::mesh::VertexAttributeValues::Float32x3(vertices) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!("particle positions should be 3D floats");
        };
        positions.push(Vec3::from_array(vertices[0]));
    }
    assert!(
        positions.windows(3).any(|three| {
            three[0].distance(three[1]) > 0.0001 && three[1].distance(three[2]) > 0.0001
        }),
        "debris should move smoothly across successive render frames, not only every 50 ms"
    );
}
