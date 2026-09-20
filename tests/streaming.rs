use std::{
    thread,
    time::{Duration, Instant},
};

use bevy::{asset::AssetPlugin, mesh::MeshPlugin, prelude::*};
use game::{
    player::Player,
    world::{
        chunk::{ChunkPos, WorldChunks},
        plugin::WorldPlugin,
    },
};

#[test]
fn distant_chunks_release_world_data_entities_and_meshes() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins(WorldPlugin);
    app.world_mut()
        .spawn((Player, Transform::from_xyz(128.0, 80.0, 0.0)));

    app.update();

    assert!(
        app.world()
            .resource::<WorldChunks>()
            .get(ChunkPos::ZERO)
            .is_none()
    );
    assert!(app.world().resource::<Assets<Mesh>>().is_empty());
    let mut chunk_positions = app.world_mut().query::<&ChunkPos>();
    assert_eq!(chunk_positions.iter(app.world()).count(), 0);

    let destination = ChunkPos { x: 8, z: 0 };
    let deadline = Instant::now() + Duration::from_secs(3);
    while app
        .world()
        .resource::<WorldChunks>()
        .get(destination)
        .is_none()
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(5));
        app.update();
    }
    assert!(
        app.world()
            .resource::<WorldChunks>()
            .get(destination)
            .is_some()
    );
    let mut chunk_positions = app.world_mut().query::<&ChunkPos>();
    assert!(
        chunk_positions
            .iter(app.world())
            .any(|position| *position == destination)
    );
}
