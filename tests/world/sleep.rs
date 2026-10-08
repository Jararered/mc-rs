//! Sleeping in a bed: `BlockBed.blockActivated`, the night skip, the spawn
//! point it sets, and the explosion where a player cannot respawn.

use bevy::prelude::*;
use game::block::blocks::Block;
use game::chat::ChatHistory;
use game::entity::EntitySize;
use game::player::Player;
use game::player::PlayerHealth;
use game::player::SurvivalPlugin;
use game::player::sleep::BED_MISSING_MESSAGE;
use game::player::sleep::BedUse;
use game::player::sleep::NO_SLEEP_MESSAGE;
use game::player::sleep::PlayerSleep;
use game::player::sleep::SPAWN_SET_MESSAGE;
use game::player::sleep::SleepPlugin;
use game::world::chunk::WorldChunks;
use game::world::dimension::ActiveDimension;
use game::world::dimension::Dimension;
use game::world::tick::DAY_LENGTH;
use game::world::tick::WorldTick;

use super::mobs::creature_app;
use super::mobs::run_ticks;
use super::pathfinding::field;

const NOON: u64 = 6000;
const MIDNIGHT: u64 = 18_000;
/// The bed's first half, and the second half sleeping resolves to.
const HEAD: IVec3 = IVec3::new(2, 61, 2);
const FOOT: IVec3 = IVec3::new(2, 61, 3);
const STANDING: Vec3 = Vec3::new(0.5, 61.0, 0.5);
/// Where waking and respawning stand the player: the first free cell.
const BESIDE_BED: Vec3 = Vec3::new(1.5, 61.1, 2.5);

/// A player on a grass field at `y = 60` beside a bed, at `time`.
fn bedroom(time: u64) -> App {
    let mut chunks = field(60);
    chunks.set_block_with_metadata(HEAD.x, HEAD.y, HEAD.z, Block::Bed, 0);
    chunks.set_block_with_metadata(FOOT.x, FOOT.y, FOOT.z, Block::Bed, 8);
    let mut app = creature_app(chunks, STANDING);
    app.add_plugins((SurvivalPlugin, SleepPlugin))
        .init_resource::<ChatHistory>();
    app.world_mut()
        .resource_mut::<WorldTick>()
        .set_world_time(time);
    app
}

fn use_bed(app: &mut App, position: IVec3) {
    app.world_mut()
        .resource_mut::<Messages<BedUse>>()
        .write(BedUse { position });
}

fn sleep(app: &mut App) -> PlayerSleep {
    *app.world_mut()
        .query::<&PlayerSleep>()
        .single(app.world())
        .unwrap()
}

fn feet(app: &mut App) -> Vec3 {
    app.world_mut()
        .query_filtered::<&Transform, With<Player>>()
        .single(app.world())
        .unwrap()
        .translation
        - Vec3::Y * EntitySize::PLAYER.y_offset
}

fn said(app: &App, text: &str) -> bool {
    app.world()
        .resource::<ChatHistory>()
        .messages()
        .any(|message| message.text == text)
}

fn block(app: &App, cell: IVec3) -> Option<Block> {
    app.world()
        .resource::<WorldChunks>()
        .block_at(cell.x, cell.y, cell.z)
}

fn metadata(app: &App, cell: IVec3) -> u8 {
    app.world()
        .resource::<WorldChunks>()
        .metadata_at(cell.x, cell.y, cell.z)
}

#[test]
fn a_bed_refuses_sleep_by_day() {
    let mut app = bedroom(NOON);
    use_bed(&mut app, HEAD);
    run_ticks(&mut app, 1);
    assert!(said(&app, NO_SLEEP_MESSAGE));
    let state = sleep(&mut app);
    assert!(!state.sleeping);
    assert_eq!(state.spawn, None);
    assert!((feet(&mut app) - STANDING).length() < 0.5);
}

#[test]
fn sleeping_skips_the_night_and_sets_the_spawn_point() {
    let mut app = bedroom(MIDNIGHT);
    // Either half works; both resolve to the second.
    use_bed(&mut app, HEAD);
    run_ticks(&mut app, 1);
    let state = sleep(&mut app);
    assert!(state.sleeping);
    assert_eq!(state.bed, Some(FOOT));
    assert_eq!(metadata(&app, FOOT), 8 | 4, "the bed is marked occupied");
    // Lying on the mattress, and held there.
    let lying = feet(&mut app) + Vec3::Y * EntitySize::PLAYER.y_offset;
    assert!((lying - Vec3::new(2.5, 61.9375, 3.9)).length() < 0.001);
    run_ticks(&mut app, 50);
    let still = feet(&mut app) + Vec3::Y * EntitySize::PLAYER.y_offset;
    assert_eq!(still, lying);
    assert!(sleep(&mut app).fade() > 0.4);
    assert!(!said(&app, SPAWN_SET_MESSAGE));

    run_ticks(&mut app, 51);
    let state = sleep(&mut app);
    assert!(!state.sleeping);
    assert_eq!(state.spawn, Some(FOOT));
    assert!(said(&app, SPAWN_SET_MESSAGE));
    assert_eq!(metadata(&app, FOOT), 8);
    let time = app.world().resource::<WorldTick>().world_time();
    assert!(
        time >= DAY_LENGTH && time % DAY_LENGTH < 20,
        "morning, not {time}"
    );
    let stood = feet(&mut app);
    assert!(
        (stood.x - BESIDE_BED.x).abs() < 0.01 && (stood.z - BESIDE_BED.z).abs() < 0.01,
        "{stood}"
    );
    // The wash fades back out.
    run_ticks(&mut app, 12);
    assert_eq!(sleep(&mut app).timer, 0);
}

#[test]
fn a_sleeper_wakes_without_a_spawn_point_when_the_bed_goes() {
    let mut app = bedroom(MIDNIGHT);
    use_bed(&mut app, FOOT);
    run_ticks(&mut app, 10);
    assert!(sleep(&mut app).sleeping);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_block(FOOT.x, FOOT.y, FOOT.z, Block::Air);
    run_ticks(&mut app, 1);
    let state = sleep(&mut app);
    assert!(!state.sleeping);
    assert_eq!((state.timer, state.spawn), (0, None));
}

#[test]
fn a_bed_explodes_where_the_player_cannot_respawn() {
    let mut app = bedroom(MIDNIGHT);
    app.world_mut().resource_mut::<ActiveDimension>().0 = Dimension::Nether;
    use_bed(&mut app, HEAD);
    run_ticks(&mut app, 5);
    assert!(!sleep(&mut app).sleeping);
    assert_eq!(block(&app, HEAD), Some(Block::Air));
    assert_eq!(block(&app, FOOT), Some(Block::Air));
    // The blast is centered one cell past the bed and digs into the floor.
    assert_ne!(block(&app, IVec3::new(2, 60, 4)), Some(Block::Grass));
    assert!(
        app.world_mut()
            .query::<&PlayerHealth>()
            .single(app.world())
            .unwrap()
            .current
            < 20
    );
}

fn die(app: &mut App) {
    app.world_mut()
        .query::<&mut PlayerHealth>()
        .single_mut(app.world_mut())
        .unwrap()
        .current = 0;
    run_ticks(app, 41);
}

#[test]
fn death_returns_the_player_to_their_bed_while_it_stands() {
    let mut app = bedroom(NOON);
    app.world_mut()
        .query::<&mut PlayerSleep>()
        .single_mut(app.world_mut())
        .unwrap()
        .spawn = Some(FOOT);
    die(&mut app);
    let stood = feet(&mut app);
    assert!(
        (stood.x - BESIDE_BED.x).abs() < 0.01 && (stood.z - BESIDE_BED.z).abs() < 0.01,
        "{stood}"
    );
    assert!(!said(&app, BED_MISSING_MESSAGE));
    assert_eq!(sleep(&mut app).spawn, Some(FOOT));

    // With the bed gone the world spawn takes over and the point is lost.
    {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        chunks.set_block(HEAD.x, HEAD.y, HEAD.z, Block::Air);
        chunks.set_block(FOOT.x, FOOT.y, FOOT.z, Block::Air);
    }
    die(&mut app);
    assert!(said(&app, BED_MISSING_MESSAGE));
    assert_eq!(sleep(&mut app).spawn, None);
    let stood = feet(&mut app);
    assert!(
        (stood.x - 8.5).abs() < 0.01 && (stood.z - 8.5).abs() < 0.01,
        "{stood}"
    );
}
