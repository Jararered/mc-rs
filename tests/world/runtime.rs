use bevy::prelude::*;
use game::world::block_ticks::BlockTicks;
use game::world::chunk::WorldChunks;
use game::world::lighting::LightCache;
use game::world::plugin::WorldPlugin;
use game::world::tick::WorldTick;

#[test]
fn world_simulation_runs_without_asset_window_or_render_plugins() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(WorldPlugin);
    app.update();
    assert!(app.world().contains_resource::<WorldChunks>());
    assert!(app.world().contains_resource::<BlockTicks>());
    assert!(app.world().contains_resource::<LightCache>());
    assert!(app.world().contains_resource::<WorldTick>());
    assert!(!app.world().contains_resource::<Assets<Mesh>>());
    assert_eq!(
        app.world_mut().query::<&Camera>().iter(app.world()).count(),
        0
    );
}
