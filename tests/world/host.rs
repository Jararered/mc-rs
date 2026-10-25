//! `WorldHost`: both dimensions of one world running at once, with players
//! moving between them.

use std::ops::Deref;
use std::ops::DerefMut;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use game::block::blocks::Block;
use game::player::PlayerName;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::dimension::ActiveDimension;
use game::world::dimension::Dimension;
use game::world::host::WorldHost;
use game::world::persistence::SaveFormat;
use game::world::persistence::WorldStorage;
use game::world::plugin::WorldPlugin;
use game::world::portal;
use game::world::streaming::WorldStreaming;
use game::world::tick::WorldTick;

use crate::temp::TempDir;

/// A host and the folder its world is saved in. Fields drop in order, so
/// the host lets go of its files before the folder is removed.
struct TestHost {
    host: WorldHost,
    _saves: TempDir,
}

impl Deref for TestHost {
    type Target = WorldHost;

    fn deref(&self) -> &WorldHost {
        &self.host
    }
}

impl DerefMut for TestHost {
    fn deref_mut(&mut self) -> &mut WorldHost {
        &mut self.host
    }
}

fn host(label: &str, format: SaveFormat) -> TestHost {
    let saves = TempDir::new(&format!("host-{label}"));
    let storage = WorldStorage::create_in_format(&saves, 7, "Hosted", None, format).unwrap();
    let host = WorldHost::new(storage, 60.0, |app| {
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    });
    TestHost {
        host,
        _saves: saves,
    }
}

fn run_until(
    host: &mut WorldHost,
    timeout: Duration,
    mut condition: impl FnMut(&mut WorldHost) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition(host) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
        host.update(0.05);
    }
}

fn eye(host: &WorldHost, dimension: Dimension, player: Entity) -> Vec3 {
    host.world(dimension)
        .unwrap()
        .get::<Transform>(player)
        .unwrap()
        .translation
}

/// The block the player's feet are in.
fn block_at_feet(host: &WorldHost, dimension: Dimension, player: Entity) -> Option<Block> {
    let feet = eye(host, dimension, player) - Vec3::Y * 1.62;
    host.world(dimension)
        .unwrap()
        .resource::<WorldChunks>()
        .block_at(
            feet.x.floor() as i32,
            (feet.y + 0.1).floor() as i32,
            feet.z.floor() as i32,
        )
}

// One test per format, so the two run side by side.
#[test]
fn two_players_can_be_in_different_dimensions_at_once() {
    two_players_apart(SaveFormat::Binary);
}

#[test]
fn two_players_can_be_in_different_dimensions_of_an_original_world() {
    two_players_apart(SaveFormat::Original);
}

fn two_players_apart(format: SaveFormat) {
    let mut host = host("apart", format);
    let (dimension, alice) = host.join("alice");
    assert_eq!(dimension, Dimension::Overworld);
    let (_, _bob) = host.join("bob");
    assert_eq!(host.loaded(), [Dimension::Overworld]);

    // Bob goes through; the teleporter builds him a portal to stand in.
    host.travel("bob");
    assert!(
        run_until(&mut host, Duration::from_secs(120), |host| {
            host.find("bob")
                .is_some_and(|(dimension, _)| dimension == Dimension::Nether)
        }),
        "{format:?}: bob never reached the Nether"
    );
    let (_, bob) = host.find("bob").unwrap();
    assert_eq!(host.find("alice"), Some((Dimension::Overworld, alice)));
    assert_eq!(host.loaded(), [Dimension::Overworld, Dimension::Nether]);
    assert_eq!(
        block_at_feet(&host, Dimension::Nether, bob),
        Some(Block::NetherPortal),
        "{format:?}"
    );
    for dimension in [Dimension::Overworld, Dimension::Nether] {
        assert_eq!(
            host.world(dimension)
                .unwrap()
                .resource::<ActiveDimension>()
                .0,
            dimension
        );
    }

    // Each dimension streams around its own player, on one clock.
    let nether_chunk = {
        let at = eye(&host, Dimension::Nether, bob);
        ChunkPosition::from_world(at.x, at.z)
    };
    assert!(
        run_until(&mut host, Duration::from_secs(60), |host| {
            let loaded = |dimension, position| {
                host.world(dimension)
                    .unwrap()
                    .resource::<WorldChunks>()
                    .contains(position)
            };
            loaded(Dimension::Overworld, ChunkPosition::ZERO)
                && loaded(
                    Dimension::Nether,
                    ChunkPosition {
                        x: nether_chunk.x + 2,
                        z: nether_chunk.z,
                    },
                )
        }),
        "{format:?}: the chunks around each player never streamed in"
    );
    let before = host.world_time();
    for _ in 0..5 {
        host.update(0.05);
    }
    assert_eq!(host.world_time(), before + 5);
    for dimension in [Dimension::Overworld, Dimension::Nether] {
        let tick = host.world(dimension).unwrap().resource::<WorldTick>();
        assert_eq!(tick.world_time(), host.world_time(), "{dimension:?}");
    }

    // Back through: the Nether has nobody left, so it saves and unloads,
    // and what the teleporter built there is on disk.
    host.travel("bob");
    assert!(
        run_until(&mut host, Duration::from_secs(120), |host| {
            host.find("bob")
                .is_some_and(|(dimension, _)| dimension == Dimension::Overworld)
        }),
        "{format:?}: bob never came back to the Overworld"
    );
    let (_, bob) = host.find("bob").unwrap();
    assert_eq!(
        block_at_feet(&host, Dimension::Overworld, bob),
        Some(Block::NetherPortal),
        "{format:?}"
    );
    assert!(
        run_until(&mut host, Duration::from_secs(180), |host| {
            host.loaded() == [Dimension::Overworld]
        }),
        "{format:?}: the empty Nether never unloaded"
    );
    let saved = host
        .storage()
        .load_chunk_in(Dimension::Nether, nether_chunk)
        .unwrap_or_else(|| panic!("{format:?}: the Nether's portal chunk was not saved"));
    assert!(portal::has_portal(&saved), "{format:?}");
}

#[test]
fn a_player_comes_back_to_the_dimension_and_place_they_left() {
    for format in [SaveFormat::Binary, SaveFormat::Original] {
        let mut host = host("records", format);
        let (dimension, carol) = host.join("carol");
        let stood = Vec3::new(40.5, 90.0, -12.5);
        host.world_mut(dimension)
            .unwrap()
            .get_mut::<Transform>(carol)
            .unwrap()
            .translation = stood;
        host.leave("carol").unwrap();
        assert_eq!(host.find("carol"), None);

        let record = host.storage().load_named_player("carol").unwrap();
        assert_eq!(record.dimension, Dimension::Overworld, "{format:?}");
        let (dimension, carol) = host.join("carol");
        assert_eq!(dimension, Dimension::Overworld);
        assert!((eye(&host, dimension, carol) - stood).length() < 0.01);
        assert_eq!(
            host.world(dimension).unwrap().get::<PlayerName>(carol),
            Some(&PlayerName("carol".to_owned()))
        );
        // A name that cannot be a file is refused rather than written.
        assert!(host.storage().load_named_player("../carol").is_none());
        assert!(
            host.storage()
                .save_named_player("../carol", &record)
                .is_err()
        );
    }
}

/// Nothing integrates a hosted player, so standing in a portal block has to
/// be noticed from where the client put them.
#[test]
fn standing_in_a_portal_sends_a_player_through() {
    let mut host = host("standing", SaveFormat::Binary);
    let (_, alice) = host.join("alice");
    let at = eye(&host, Dimension::Overworld, alice);
    let cell = IVec3::new(
        at.x.floor() as i32,
        (at.y - 1.62 + 0.1).floor() as i32,
        at.z.floor() as i32,
    );
    // Finished, so that no population pass is still to write into it.
    assert!(
        run_until(&mut host, Duration::from_secs(60), |host| {
            let world = host.world(Dimension::Overworld).unwrap();
            let chunks = world.resource::<WorldChunks>();
            world
                .get_resource::<WorldStreaming>()
                .is_some_and(|streaming| {
                    streaming
                        .neighborhood_finished(chunks, ChunkPosition::from_block(cell.x, cell.z))
                })
        }),
        "the chunks around the player never finished"
    );
    // A new arrival cannot start a trip until `timeUntilPortal` has run out.
    for _ in 0..30 {
        host.update(0.05);
    }
    // A whole portal, in its frame: a portal block on its own is taken away
    // by the first neighbour that changes.
    let mut chunks = host
        .world_mut(Dimension::Overworld)
        .unwrap()
        .resource_mut::<WorldChunks>();
    for width in -1..=2 {
        for height in -1..=3 {
            let edge = width == -1 || width == 2 || height == -1 || height == 3;
            chunks.set_block(
                cell.x + width,
                cell.y + height,
                cell.z,
                if edge {
                    Block::Obsidian
                } else {
                    Block::NetherPortal
                },
            );
        }
    }
    assert!(
        run_until(&mut host, Duration::from_secs(120), |host| {
            host.find("alice")
                .is_some_and(|(dimension, _)| dimension == Dimension::Nether)
        }),
        "a player standing in a portal never travelled"
    );
}

#[test]
fn a_remote_rider_in_a_portal_is_put_off_its_boat() {
    use game::entity::boat::Boat;
    use game::entity::boat::spawn_boat_at;
    use game::entity::mount::Mounted;
    use game::entity::mount::mount;

    let mut host = host("ride-portal", SaveFormat::Binary);
    let (dimension, dave) = host.join("dave");
    // Past the cooldown a fresh arrival starts with, and the spawn chunk
    // streamed in.
    for _ in 0..30 {
        host.update(0.05);
    }
    assert!(
        run_until(&mut host, Duration::from_secs(60), |host| {
            host.world(dimension)
                .unwrap()
                .resource::<WorldChunks>()
                .contains(ChunkPosition::ZERO)
        }),
        "the spawn chunk never streamed in"
    );
    let feet = eye(&host, dimension, dave) - Vec3::Y * 1.62;
    let world = host.world_mut(dimension).unwrap();
    let boat = spawn_boat_at(&mut world.commands(), feet + Vec3::Y * 0.3, Boat::default());
    world.flush();
    mount(&mut world.commands(), dave, boat);
    world.flush();
    assert!(world.get::<Mounted>(dave).is_some());
    // Portal blocks all round the seat, untouched by block updates.
    {
        let mut chunks = world.resource_mut::<WorldChunks>();
        let cell = feet.floor().as_ivec3();
        for x in -1..=1 {
            for y in 0..=3 {
                for z in -1..=1 {
                    let placed =
                        chunks.set_block(cell.x + x, cell.y + y, cell.z + z, Block::NetherPortal);
                    assert!(placed.is_some(), "{cell} {feet}");
                }
            }
        }
    }
    assert!(
        run_until(&mut host, Duration::from_secs(5), |host| {
            host.world(dimension)
                .unwrap()
                .get::<Mounted>(dave)
                .is_none()
        }),
        "the rider was never put off the boat"
    );
    assert_eq!(host.find("dave"), Some((dimension, dave)));
}
