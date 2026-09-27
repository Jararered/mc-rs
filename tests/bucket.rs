//! `ItemBucket`: an empty bucket takes a water or lava source out of the
//! world, and a filled one pours a source back in.

use std::time::Duration;

use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;
use game::app::settings::GameSettings;
use game::app::state::AppScreen;
use game::block::id::Id;
use game::inventory::Hotbar;
use game::item::ItemId;
use game::item::ItemStack;
use game::player::Player;
use game::player::PlayerPlugin;
use game::player::use_bucket;
use game::ui::HudPlugin;
use game::ui::InventoryGuiPlugin;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;
use game::world::tick::WorldTick;
use game::world::tick::advance_world_tick;

fn generated(chunk: Chunk) -> GeneratedChunk {
    GeneratedChunk {
        heightmap: Heightmap::from_chunk(&chunk),
        biomes: BiomeMap::from_cells(
            [Climate {
                temperature: 0.5,
                humidity: 0.5,
                biome: Biome::Plains,
            }; CHUNK_SIZE * CHUNK_SIZE],
        ),
        chunk,
        items: Vec::new(),
        populated: true,
    }
}

fn world_with(chunk: Chunk) -> WorldChunks {
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));
    chunks
}

fn bucket(item: ItemId) -> ItemStack {
    ItemStack::new(item, 1).expect("buckets are registered with a stack size of one")
}

/// Look straight down the −Z axis from the middle of a column.
fn looking_north() -> (Vec3, Vec3) {
    (Vec3::new(8.5, 64.5, 10.5), Vec3::NEG_Z)
}

fn block(chunks: &WorldChunks, x: i32, y: i32, z: i32) -> Id {
    chunks.block_at(x, y, z).expect("test cells are loaded")
}

#[test]
fn empty_bucket_takes_a_water_source() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::Water);
    let mut chunks = world_with(chunk);
    let (origin, direction) = looking_north();

    let used = use_bucket(&mut chunks, origin, direction, bucket(ItemId::Bucket))
        .expect("a water source is scoopable");

    assert_eq!(block(&chunks, 8, 64, 8), Id::Air);
    assert_eq!(used.result.item(), ItemId::WaterBucket);
    assert_eq!(used.previous, Id::Water);
    assert_eq!(used.position, bevy::math::IVec3::new(8, 64, 8));
}

#[test]
fn empty_bucket_takes_a_lava_source() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::Lava);
    let mut chunks = world_with(chunk);
    let (origin, direction) = looking_north();

    let used = use_bucket(&mut chunks, origin, direction, bucket(ItemId::Bucket))
        .expect("a lava source is scoopable");

    assert_eq!(block(&chunks, 8, 64, 8), Id::Air);
    assert_eq!(used.result.item(), ItemId::LavaBucket);
}

/// `BlockFluid.canCollideCheck` is `hitFluids && metadata == 0`, so spread
/// fluid is see-through even to an empty bucket.
#[test]
fn empty_bucket_will_not_take_spread_water() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::FlowingWater);
    chunk.set_metadata(8, 64, 8, 1);
    let mut chunks = world_with(chunk);
    let (origin, direction) = looking_north();

    assert!(use_bucket(&mut chunks, origin, direction, bucket(ItemId::Bucket)).is_none());
    assert_eq!(block(&chunks, 8, 64, 8), Id::FlowingWater);
    assert_eq!(chunks.metadata_at(8, 64, 8), 1);
}

/// Falling fluid carries the level above eight and reads as a source to
/// `percent_air`, but its raw metadata is not zero, so it is not scoopable.
#[test]
fn empty_bucket_will_not_take_falling_water() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::FlowingWater);
    chunk.set_metadata(8, 64, 8, 8);
    let mut chunks = world_with(chunk);
    let (origin, direction) = looking_north();

    assert!(use_bucket(&mut chunks, origin, direction, bucket(ItemId::Bucket)).is_none());
    assert_eq!(block(&chunks, 8, 64, 8), Id::FlowingWater);
}

#[test]
fn empty_bucket_does_nothing_to_air_or_stone() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 6, Id::Stone);
    let mut chunks = world_with(chunk);
    let (origin, direction) = looking_north();

    assert!(use_bucket(&mut chunks, origin, direction, bucket(ItemId::Bucket)).is_none());
    assert_eq!(block(&chunks, 8, 64, 6), Id::Stone);
}

/// A non-bucket stack is left alone entirely.
#[test]
fn non_bucket_items_are_not_intercepted() {
    let mut chunks = world_with(Chunk::new());
    let (origin, direction) = looking_north();

    assert!(use_bucket(&mut chunks, origin, direction, bucket(ItemId::IronIngot)).is_none());
    assert!(use_bucket(&mut chunks, origin, direction, bucket(ItemId::MilkBucket)).is_none());
}

/// A filled bucket looks through the pool and fills the cell the ray enters
/// last, which is what makes it usable on a pool's floor.
///
/// The ray runs north from `z = 10.5`. Water at `z = 8` is skipped, so the
/// stone behind it is the hit and the fill lands on the stone's south face at
/// `z = 8`. Had the ray stopped on the water, the fill would have gone to
/// `z = 9` instead.
#[test]
fn a_filled_bucket_looks_through_a_pool() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::Water);
    chunk.set(8, 64, 7, Id::Stone);
    let mut chunks = world_with(chunk);
    let (origin, direction) = looking_north();

    let used = use_bucket(&mut chunks, origin, direction, bucket(ItemId::WaterBucket))
        .expect("the far wall of the pool is in reach");

    assert_eq!(used.position, bevy::math::IVec3::new(8, 64, 8));
    assert_eq!(used.previous, Id::Water);
    assert_eq!(block(&chunks, 8, 64, 8), Id::FlowingWater);
    assert_eq!(chunks.metadata_at(8, 64, 8), 0);
    assert_eq!(used.result.item(), ItemId::Bucket);
}

/// `isFull` is the block id to place, so a bucket writes the *flowing* id and
/// the tick pass hardens it.
#[test]
fn a_poured_bucket_leaves_the_flowing_block() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::Stone);
    let mut chunks = world_with(chunk);
    let (origin, direction) = looking_north();

    let used = use_bucket(&mut chunks, origin, direction, bucket(ItemId::LavaBucket))
        .expect("a block face is fillable");

    // The ray runs north and enters the stone through its south face, so the
    // fill lands on the player's side of the block.
    assert_eq!(used.position, bevy::math::IVec3::new(8, 64, 9));
    assert_eq!(block(&chunks, 8, 64, 9), Id::FlowingLava);
    assert_eq!(used.previous, Id::Air);
    assert_eq!(used.result.item(), ItemId::Bucket);
}

/// A ray that skims just over a lowered block does not stop on it, so that
/// block becomes the cell the bucket would fill. A solid material there is
/// refused, as `isAirBlock || !isSolid` requires.
#[test]
fn a_bucket_will_not_pour_over_a_solid_material() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::Farmland);
    chunk.set(8, 64, 7, Id::Stone);
    let mut chunks = world_with(chunk);
    // Farmland's selection top sits at 15/16, so the ray passes over it.
    let origin = Vec3::new(8.5, 64.97, 10.5);

    assert!(
        use_bucket(
            &mut chunks,
            origin,
            Vec3::NEG_Z,
            bucket(ItemId::WaterBucket)
        )
        .is_none()
    );
    assert_eq!(block(&chunks, 8, 64, 8), Id::Farmland);
}

/// The same ray over a non-solid lowered block, which the bucket may replace.
#[test]
fn a_bucket_pours_over_a_non_solid_material() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::SnowLayer);
    chunk.set(8, 64, 7, Id::Stone);
    let mut chunks = world_with(chunk);
    let origin = Vec3::new(8.5, 64.97, 10.5);

    let used = use_bucket(
        &mut chunks,
        origin,
        Vec3::NEG_Z,
        bucket(ItemId::WaterBucket),
    )
    .expect("a snow layer is not a solid material");

    assert_eq!(used.position, bevy::math::IVec3::new(8, 64, 8));
    assert_eq!(used.previous, Id::SnowLayer);
    assert_eq!(block(&chunks, 8, 64, 8), Id::FlowingWater);
}

/// Liquids are not solid materials, so a bucket may pour over them. Plants
/// and torches are non-solid too, but the ray stops on those first, so they
/// end up behind the fill rather than replaced by it.
#[test]
fn a_bucket_replaces_liquids() {
    for replaceable in [Id::Water, Id::FlowingWater, Id::Lava, Id::FlowingLava] {
        let mut chunk = Chunk::new();
        chunk.set(8, 64, 7, Id::Stone);
        chunk.set(8, 64, 8, replaceable);
        let mut chunks = world_with(chunk);
        let (origin, direction) = looking_north();

        let used = use_bucket(&mut chunks, origin, direction, bucket(ItemId::WaterBucket))
            .unwrap_or_else(|| panic!("{replaceable:?} should be replaceable"));
        assert_eq!(used.previous, replaceable, "{replaceable:?}");
        assert_eq!(
            block(&chunks, 8, 64, 8),
            Id::FlowingWater,
            "{replaceable:?}"
        );
    }
}

/// Beta discards what `setBlockAndMetadataWithNotify` returns and hands back
/// an empty bucket regardless, so pouring onto the same fluid spends the
/// bucket and changes nothing.
#[test]
fn pouring_onto_the_same_fluid_still_spends_the_bucket() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 7, Id::Stone);
    chunk.set(8, 64, 8, Id::FlowingWater);
    chunk.set_metadata(8, 64, 8, 0);
    let mut chunks = world_with(chunk);
    let (origin, direction) = looking_north();

    let used = use_bucket(&mut chunks, origin, direction, bucket(ItemId::WaterBucket))
        .expect("a liquid cell is fillable");

    assert_eq!(used.previous, Id::FlowingWater);
    assert_eq!(block(&chunks, 8, 64, 8), Id::FlowingWater);
    assert_eq!(used.result.item(), ItemId::Bucket);
}

#[test]
fn a_bucket_reaches_no_further_than_the_block_reach() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 2, Id::Water);
    let mut chunks = world_with(chunk);
    let (origin, direction) = looking_north();

    assert!(use_bucket(&mut chunks, origin, direction, bucket(ItemId::Bucket)).is_none());
    assert_eq!(block(&chunks, 8, 64, 2), Id::Water);
}

#[test]
fn a_bucket_ignores_an_unloaded_chunk() {
    let mut chunks = WorldChunks::default();
    let (origin, direction) = looking_north();

    assert!(use_bucket(&mut chunks, origin, direction, bucket(ItemId::Bucket)).is_none());
}

// --- The real interaction system -------------------------------------------

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        StatesPlugin,
        bevy::input::InputPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
    )))
    .init_asset::<Image>()
    .init_asset::<Font>()
    .init_state::<AppScreen>()
    .init_resource::<GameSettings>()
    .init_resource::<WorldChunks>()
    .init_resource::<WorldTick>()
    .add_systems(First, advance_world_tick)
    .add_plugins((PlayerPlugin, InventoryGuiPlugin, HudPlugin));
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();
    app
}

/// A player with a focused, locked cursor, standing still and level at the
/// middle of chunk (0, 0).
fn playing_app() -> App {
    let mut app = app();
    app.world_mut().spawn((
        Window {
            focused: true,
            ..default()
        },
        CursorOptions {
            grab_mode: CursorGrabMode::Locked,
            ..default()
        },
        PrimaryWindow,
    ));
    // Freeze the player at eye height facing one way along an axis, so the
    // geometry below works whichever way `look_player` settles on.
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Transform, With<Player>>();
    let mut transform = query.single_mut(app.world_mut()).expect("one player");
    transform.translation = Vec3::new(8.5, 65.62, 8.5);
    transform.rotation = Quat::IDENTITY;
    app
}

fn load(app: &mut App, chunk: Chunk) {
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .insert(ChunkPosition::ZERO, generated(chunk));
}

fn hold(app: &mut App, item: ItemId) {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Hotbar, With<Player>>();
    let mut bar = query.single_mut(app.world_mut()).expect("one player");
    bar.slots[0] = Some(bucket(item));
}

fn held(app: &mut App) -> Option<ItemId> {
    let mut query = app.world_mut().query_filtered::<&Hotbar, With<Player>>();
    let bar = query.single(app.world()).expect("one player");
    bar.slots[0].map(|stack| stack.item())
}

fn right_click(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Right);
    app.update();
}

fn count(app: &App, id: Id) -> usize {
    let chunks = app.world().resource::<WorldChunks>();
    (0..CHUNK_SIZE)
        .flat_map(|x| (0..CHUNK_SIZE).map(move |z| (x, z)))
        .flat_map(|(x, z)| (0..128).map(move |y| (x, y, z)))
        .filter(|&(x, y, z)| chunks.block_at(x as i32, y, z as i32) == Some(id))
        .count()
}

/// Two water columns flanking the player, so a test can aim at one whichever
/// way the camera faces.
fn pool_around_player() -> Chunk {
    let mut chunk = Chunk::new();
    for z in [2, 7, 9, 14] {
        for x in 4..12 {
            for y in 60..70 {
                chunk.set(x, y, z, Id::Water);
            }
        }
    }
    chunk
}

/// The eye sits in the open cell (8, 65, 8). Targets go two cells away on
/// each side so the cell a block lands in is not the player's own cell, which
/// `place_selected_block_facing` refuses. Both sides are populated so the
/// tests do not depend on which way the camera faces.
fn setup(app: &mut App, near: Id, far: Option<Id>) {
    let mut chunk = pool_around_player();
    for z in [6, 10] {
        chunk.set(8, 65, z, near);
    }
    for (z, block) in [(5, far), (11, far)] {
        if let Some(block) = block {
            chunk.set(8, 65, z, block);
        }
    }
    load(app, chunk);
}

/// The control: an ordinary block item on the same right-click. If this
/// places, `interact_blocks` is running and a bucket that does nothing is a
/// bucket bug; if it does not, the harness is not driving the system.
#[test]
fn right_click_with_a_plain_block_places_it() {
    let mut app = playing_app();
    setup(&mut app, Id::Stone, None);
    hold(&mut app, ItemId::from_block(Id::Stone).unwrap());
    assert_eq!(count(&app, Id::Stone), 2);

    right_click(&mut app);

    assert_eq!(count(&app, Id::Stone), 3, "a block was placed");
}

#[test]
fn right_click_with_an_empty_bucket_takes_water() {
    let mut app = playing_app();
    setup(&mut app, Id::Water, None);
    hold(&mut app, ItemId::Bucket);
    assert_eq!(count(&app, Id::Water), 322);

    right_click(&mut app);

    assert_eq!(held(&mut app), Some(ItemId::WaterBucket));
    assert_eq!(count(&app, Id::Water), 321, "one cell was scooped");
}

#[test]
fn right_click_with_a_water_bucket_pours_water_back() {
    let mut app = playing_app();
    setup(&mut app, Id::Air, None);
    // The ray looks through the water column and stops on the wall behind it.
    let mut chunk = pool_around_player();
    for z in [5, 11] {
        chunk.set(8, 65, z, Id::Water);
    }
    for z in [6, 10] {
        chunk.set(8, 65, z, Id::Stone);
    }
    load(&mut app, chunk);
    hold(&mut app, ItemId::WaterBucket);
    assert_eq!(count(&app, Id::FlowingWater), 0);

    right_click(&mut app);

    assert_eq!(held(&mut app), Some(ItemId::Bucket));
    assert_eq!(count(&app, Id::FlowingWater), 1, "a cell was filled");
}
