//! `WorldHost`: both dimensions of one world running at once, with players
//! moving between them.

use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

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
use game::world::tick::WorldTick;

fn host(label: &str, format: SaveFormat) -> WorldHost {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let saves = std::env::temp_dir().join(format!("game-host-{label}-{unique}"));
    std::fs::create_dir_all(&saves).unwrap();
    let storage = WorldStorage::create_in_format(&saves, 7, "Hosted", None, format).unwrap();
    WorldHost::new(storage, 60.0, |app| {
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    })
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

#[test]
fn two_players_can_be_in_different_dimensions_at_once() {
    for format in [SaveFormat::Binary, SaveFormat::Original] {
        let mut host = host("apart", format);
        let (dimension, alice) = host.join("alice");
        assert_eq!(dimension, Dimension::Overworld);
        let (_, _bob) = host.join("bob");
        assert_eq!(host.loaded(), [Dimension::Overworld]);

        // Bob goes through; the teleporter builds him a portal to stand in.
        host.travel("bob");
        assert!(run_until(&mut host, Duration::from_secs(120), |host| {
            host.find("bob")
                .is_some_and(|(dimension, _)| dimension == Dimension::Nether)
        }));
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
        assert!(run_until(&mut host, Duration::from_secs(60), |host| {
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
        }));
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
        assert!(run_until(&mut host, Duration::from_secs(120), |host| {
            host.find("bob")
                .is_some_and(|(dimension, _)| dimension == Dimension::Overworld)
        }));
        let (_, bob) = host.find("bob").unwrap();
        assert_eq!(
            block_at_feet(&host, Dimension::Overworld, bob),
            Some(Block::NetherPortal),
            "{format:?}"
        );
        assert!(
            run_until(&mut host, Duration::from_secs(60), |host| {
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
    assert!(run_until(&mut host, Duration::from_secs(60), |host| {
        host.world(Dimension::Overworld)
            .unwrap()
            .resource::<WorldChunks>()
            .block_at(cell.x, cell.y, cell.z)
            .is_some()
    }));
    // A new arrival cannot start a trip until `timeUntilPortal` has run out.
    for _ in 0..30 {
        host.update(0.05);
    }
    host.world_mut(Dimension::Overworld)
        .unwrap()
        .resource_mut::<WorldChunks>()
        .set_block(cell.x, cell.y, cell.z, Block::NetherPortal);
    assert!(
        run_until(&mut host, Duration::from_secs(120), |host| {
            host.find("alice")
                .is_some_and(|(dimension, _)| dimension == Dimension::Nether)
        }),
        "a player standing in a portal never travelled"
    );
}
