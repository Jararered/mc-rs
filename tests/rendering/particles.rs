use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use game::app::state::AppScreen;
use game::block::blocks::Block;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::rendering::particles::block::BlockParticlePlugin;
use game::rendering::particles::block::BlockParticles;
use game::rendering::particles::display::random_display_updates;
use game::rendering::particles::effects::EffectParticles;
use game::rendering::particles::effects::FxKind;
use game::rendering::particles::registry::ParticleSprite;
use game::rendering::textures::FoliageColors;
use game::rendering::textures::GrassColors;
use game::rendering::textures::PALETTE_SIZE;
use game::rendering::textures::atlas_tile_uvs;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::chunk::Chunk;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;
use game::world::tick::WorldTick;
use game::world::tick::advance_world_tick;

#[test]
fn particle_registry_maps_named_sprites_to_their_atlas_tiles() {
    assert_eq!(ParticleSprite::Explosion(7).tile(), Some((7, 0)));
    assert_eq!(ParticleSprite::WaterSplash(2).tile(), Some((3, 1)));
    assert_eq!(ParticleSprite::WaterSplash(5).tile(), Some((6, 1)));
    assert_eq!(ParticleSprite::AirBubble.tile(), Some((0, 2)));
    assert_eq!(ParticleSprite::FishingLure.tile(), Some((1, 2)));
    assert_eq!(ParticleSprite::Flame.tile(), Some((0, 3)));
    assert_eq!(ParticleSprite::Lava.tile(), Some((1, 3)));
    assert_eq!(ParticleSprite::MusicNote.tile(), Some((0, 4)));
    assert_eq!(ParticleSprite::HealthHeart.tile(), Some((0, 5)));
    assert_eq!(ParticleSprite::SoulSandStep(1).tile(), Some((1, 6)));
    assert_eq!(ParticleSprite::Explosion(8).tile(), None);
    assert_eq!(ParticleSprite::WaterSplash(6).tile(), None);
    assert_eq!(ParticleSprite::SoulSandStep(2).tile(), None);

    let tiles: std::collections::HashSet<_> = ParticleSprite::ALL
        .iter()
        .map(|sprite| sprite.tile().expect("listed sprite must be valid"))
        .collect();
    assert_eq!(tiles.len(), ParticleSprite::ALL.len());
}

#[test]
fn particle_registry_provides_pixel_and_uv_bounds() {
    assert_eq!(
        ParticleSprite::FishingLure.pixel_rect(),
        Some(Rect::new(8.0, 16.0, 16.0, 24.0))
    );
    assert_eq!(
        ParticleSprite::FishingLure.uvs(),
        Some((1.0 / 16.0, 2.0 / 16.0, 2.0 / 16.0, 3.0 / 16.0))
    );
    assert_eq!(ParticleSprite::WaterSplash(6).pixel_rect(), None);
}

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
    .init_resource::<WorldTick>()
    .add_systems(First, advance_world_tick)
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
        block: Block::Stone,
    }
}

fn palette(rgb: [u8; 3]) -> Vec<u8> {
    [rgb[0], rgb[1], rgb[2], 255].repeat(PALETTE_SIZE * PALETTE_SIZE)
}

fn add_climate_chunk(app: &mut App) {
    let chunk = Chunk::new();
    let climate = Climate {
        temperature: 0.5,
        humidity: 0.5,
        biome: Biome::Forest,
    };
    app.world_mut().resource_mut::<WorldChunks>().insert(
        game::world::chunk::ChunkPosition::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            chunk,
            biomes: BiomeMap::from_cells([climate; 16 * 16]),
            items: Vec::new(),
            populated: true,
        },
    );
}

fn particle_colors(app: &mut App) -> Vec<[f32; 4]> {
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
    let bevy::mesh::VertexAttributeValues::Float32x4(colors) =
        mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
    else {
        panic!("particle colors should be float RGBA values");
    };
    colors.clone()
}

#[test]
fn tall_grass_and_fern_break_and_hit_particles_use_biome_grass_tint() {
    for (block, metadata) in [(Block::TallGrass, 0), (Block::TallGrass, 2)] {
        for breaking in [true, false] {
            let mut app = test_app();
            add_climate_chunk(&mut app);
            app.insert_resource(GrassColors::from_rgba(palette([255, 0, 0])));
            let hit = BlockHit {
                block,
                ..stone_hit()
            };
            if breaking {
                app.world_mut()
                    .resource_mut::<BlockParticles>()
                    .emit_break_state(hit, metadata);
            } else {
                app.world_mut()
                    .resource_mut::<BlockParticles>()
                    .emit_hit(hit);
            }
            app.update();

            let colors = particle_colors(&mut app);
            assert!(!colors.is_empty());
            assert!(
                colors
                    .iter()
                    .all(|color| { *color == [0.6, 0.0, 0.0, 1.0] })
            );
        }
    }
}

#[test]
fn non_grass_particles_keep_neutral_or_foliage_tint() {
    let mut app = test_app();
    add_climate_chunk(&mut app);
    app.insert_resource(GrassColors::from_rgba(palette([255, 0, 0])));
    app.insert_resource(FoliageColors::from_rgba(palette([0, 0, 255])));

    app.world_mut()
        .resource_mut::<BlockParticles>()
        .emit_hit(stone_hit());
    app.update();
    assert!(
        particle_colors(&mut app)
            .iter()
            .all(|color| *color == [0.6, 0.6, 0.6, 1.0])
    );

    app.world_mut()
        .resource_mut::<BlockParticles>()
        .emit_hit(BlockHit {
            block: Block::Leaves,
            ..stone_hit()
        });
    app.update();
    let colors = particle_colors(&mut app);
    assert!(
        colors[4..]
            .iter()
            .all(|color| *color == [0.0, 0.0, 0.6, 1.0])
    );
}

#[test]
fn grass_particles_use_standard_tint_when_climate_and_palette_are_unavailable() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<BlockParticles>()
        .emit_hit(BlockHit {
            block: Block::TallGrass,
            ..stone_hit()
        });
    app.update();

    let colors = particle_colors(&mut app);
    assert!(
        colors
            .iter()
            .all(|color| *color == [0.55 * 0.6, 0.8 * 0.6, 0.4 * 0.6, 1.0])
    );
}

#[test]
fn block_break_spawns_sixty_four_textured_billboards_and_expires() {
    let mut app = test_app();
    {
        let mut renderers = app.world_mut().query::<(&Name, &Mesh3d, &Visibility)>();
        let (_, handle, visible) = renderers
            .iter(app.world())
            .find(|(name, _, _)| name.as_str() == "Block particles")
            .unwrap();
        assert_eq!(*visible, Visibility::Hidden);
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&handle.0)
                .unwrap()
                .count_vertices()
                > 0,
            "an idle particle renderer must never upload an empty mesh"
        );
    }
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
    let mut renderers = app.world_mut().query::<(&Name, &Mesh3d, &Visibility)>();
    let (_, handle, visible) = renderers
        .iter(app.world())
        .find(|(name, _, _)| name.as_str() == "Block particles")
        .unwrap();
    assert_eq!(*visible, Visibility::Hidden);
    assert!(
        app.world()
            .resource::<Assets<Mesh>>()
            .get(&handle.0)
            .unwrap()
            .count_vertices()
            > 0,
        "expired particles should hide the mesh rather than upload zero vertices"
    );

    app.world_mut()
        .resource_mut::<BlockParticles>()
        .emit_break(stone_hit());
    app.update();
    assert_eq!(app.world().resource::<BlockParticles>().active_count(), 64);
    let mut renderers = app.world_mut().query::<(&Name, &Visibility)>();
    let (_, visible) = renderers
        .iter(app.world())
        .find(|(name, _)| name.as_str() == "Block particles")
        .unwrap();
    assert_eq!(*visible, Visibility::Visible);
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

/// One chunk of `biome` with a stone floor, optionally roofed over.
fn rain_world(biome: Biome, roofed: bool) -> WorldChunks {
    let mut chunk = Chunk::new();
    for x in 0..16 {
        for z in 0..16 {
            chunk.set(x, 63, z, Block::Stone);
            if roofed {
                chunk.set(x, 100, z, Block::Stone);
            }
        }
    }
    let climate = Climate {
        temperature: 0.5,
        humidity: 0.5,
        biome,
    };
    let mut chunks = WorldChunks::default();
    chunks.insert(
        game::world::chunk::ChunkPosition::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            chunk,
            biomes: BiomeMap::from_cells([climate; 16 * 16]),
            items: Vec::new(),
            populated: true,
        },
    );
    chunks
}

fn splashes(chunks: &WorldChunks, strength: f32, fancy: bool) -> usize {
    let mut rain = EffectParticles::default();
    rain.spawn_rain(chunks, Vec3::new(8.5, 65.6, 8.5), strength, fancy);
    rain.active_count()
}

#[test]
fn rain_splashes_land_on_exposed_ground_in_raining_biomes() {
    let open = rain_world(Biome::Forest, false);
    let full = splashes(&open, 1.0, true);
    // A hundred tries a tick; the ones that land outside the chunk are lost.
    assert!((20..=100).contains(&full), "{full} splashes");
    assert_eq!(splashes(&open, 0.0, true), 0);
    // Fast graphics halves the strength, and the count goes with its square.
    assert!(splashes(&open, 1.0, false) <= 25);

    // The roof is the top solid block and it is out of reach overhead.
    assert_eq!(splashes(&rain_world(Biome::Forest, true), 1.0, true), 0);
    assert_eq!(splashes(&rain_world(Biome::Desert, false), 1.0, true), 0);
    assert_eq!(splashes(&rain_world(Biome::Tundra, false), 1.0, true), 0);
}

/// A chunk with a stone floor at y=63 and `block` (with `metadata`) on top.
fn world_with(block: Block, metadata: u8, position: (usize, usize, usize)) -> WorldChunks {
    let mut chunk = Chunk::new();
    for x in 0..16 {
        for z in 0..16 {
            chunk.set(x, 63, z, Block::Stone);
        }
    }
    chunk.set(position.0, position.1, position.2, block);
    chunk.set_metadata(position.0, position.1, position.2, metadata);
    let mut chunks = WorldChunks::default();
    chunks.insert(
        game::world::chunk::ChunkPosition::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            chunk,
            biomes: BiomeMap::from_cells(
                [Climate {
                    temperature: 0.5,
                    humidity: 0.5,
                    biome: Biome::Forest,
                }; 16 * 16],
            ),
            items: Vec::new(),
            populated: true,
        },
    );
    chunks
}

#[test]
fn spawn_ignores_effects_beyond_sixteen_blocks_of_the_viewer() {
    let mut particles = EffectParticles::default();
    particles.set_viewer(Some(Vec3::ZERO));
    particles.spawn(FxKind::Smoke, Vec3::new(15.9, 0.0, 0.0), Vec3::ZERO);
    assert_eq!(particles.active_count(), 1);
    particles.spawn(FxKind::Smoke, Vec3::new(16.1, 0.0, 0.0), Vec3::ZERO);
    particles.spawn(FxKind::Flame, Vec3::new(0.0, 0.0, -20.0), Vec3::ZERO);
    assert_eq!(particles.active_count(), 1);
    particles.set_viewer(None);
    particles.spawn(FxKind::Flame, Vec3::new(0.0, 0.0, -20.0), Vec3::ZERO);
    assert_eq!(particles.active_count(), 2);
}

#[test]
fn every_effect_kind_spawns_and_eventually_dies() {
    let chunks = WorldChunks::default();
    for kind in [
        FxKind::Bubble,
        FxKind::Smoke,
        FxKind::LargeSmoke,
        FxKind::Note,
        FxKind::Portal,
        FxKind::Explode,
        FxKind::Flame,
        FxKind::Lava,
        FxKind::Splash,
        FxKind::Reddust,
        FxKind::Heart,
        FxKind::SnowballPoof,
        FxKind::Slime,
    ] {
        let mut particles = EffectParticles::default();
        particles.spawn(kind, Vec3::new(8.0, 70.0, 8.0), Vec3::new(0.0, 0.1, 0.0));
        assert!(particles.active_count() >= 1, "{kind:?} did not spawn");
        for _ in 0..400 {
            particles.tick(&chunks, None, 0);
        }
        assert_eq!(particles.active_count(), 0, "{kind:?} outlived its age");
    }
}

#[test]
fn bubbles_pop_when_they_leave_the_water() {
    let mut particles = EffectParticles::default();
    let chunks = world_with(Block::Water, 0, (8, 64, 8));
    particles.spawn(FxKind::Bubble, Vec3::new(8.5, 64.5, 8.5), Vec3::ZERO);
    particles.tick(&chunks, None, 0);
    assert_eq!(particles.active_count(), 1);
    // Nothing but air above the single water block: it floats out and dies.
    for _ in 0..40 {
        particles.tick(&chunks, None, 0);
    }
    assert_eq!(particles.active_count(), 0);
}

#[test]
fn lava_pops_throw_smoke_while_they_fall() {
    let mut particles = EffectParticles::default();
    let chunks = world_with(Block::Air, 0, (0, 70, 0));
    particles.spawn(FxKind::Lava, Vec3::new(8.5, 70.0, 8.5), Vec3::ZERO);
    let mut most = 0;
    for _ in 0..40 {
        particles.tick(&chunks, None, 0);
        most = most.max(particles.active_count());
    }
    assert!(most > 1, "a lava pop should leave smoke behind");
}

#[test]
fn explosions_leave_a_puff_and_smoke_for_each_destroyed_cell() {
    let mut particles = EffectParticles::default();
    particles.blast_cell(IVec3::new(8, 65, 8), Vec3::new(8.5, 64.5, 8.5), 4.0);
    assert_eq!(particles.active_count(), 2);
}

#[test]
fn entity_bursts_match_beta_counts() {
    let mut particles = EffectParticles::default();
    particles.death_puffs(Vec3::new(8.0, 65.0, 8.0), 0.6, 1.8);
    assert_eq!(particles.active_count(), 20);
    let mut particles = EffectParticles::default();
    particles.tame_burst(Vec3::new(8.0, 65.0, 8.0), 0.6, 0.8, true);
    assert_eq!(particles.active_count(), 7);
    let mut particles = EffectParticles::default();
    particles.drown(Vec3::new(8.0, 65.0, 8.0), Vec3::ZERO);
    assert_eq!(particles.active_count(), 8);
    let mut particles = EffectParticles::default();
    // 1 + 0.6 * 20 = 13 bubbles and as many splashes.
    particles.water_entry(
        Vec3::new(8.0, 65.0, 8.0),
        64.0,
        0.6,
        Vec3::new(0.0, -0.3, 0.0),
    );
    assert_eq!(particles.active_count(), 26);
}

fn display_count(chunks: &WorldChunks, ticks: usize) -> usize {
    let mut particles = EffectParticles::default();
    for _ in 0..ticks {
        random_display_updates(&mut particles, chunks, IVec3::new(8, 65, 8));
    }
    particles.active_count()
}

#[test]
fn torches_furnaces_and_portals_make_display_particles() {
    assert_eq!(display_count(&world_with(Block::Air, 0, (0, 70, 0)), 50), 0);
    for (block, metadata) in [
        (Block::Torch, 0),
        (Block::Torch, 1),
        (Block::LitFurnace, 4),
        (Block::Fire, 0),
        (Block::NetherPortal, 0),
        (Block::RedstoneTorch, 0),
        (Block::LitRedstoneOre, 0),
        (Block::RedstoneWire, 15),
        (Block::PoweredRepeater, 0),
    ] {
        let chunks = world_with(block, metadata, (8, 64, 8));
        assert!(
            display_count(&chunks, 200) > 0,
            "{block:?} made no display particles"
        );
    }
    // An unlit furnace, an unpowered wire and a plain stone block stay quiet.
    for (block, metadata) in [
        (Block::Furnace, 4),
        (Block::RedstoneWire, 0),
        (Block::Cobblestone, 0),
        (Block::UnlitRedstoneTorch, 0),
    ] {
        let chunks = world_with(block, metadata, (8, 64, 8));
        assert_eq!(display_count(&chunks, 200), 0, "{block:?} should be quiet");
    }
}

#[test]
fn a_fast_boat_throws_a_wake_and_a_slow_one_does_not() {
    let mut particles = EffectParticles::default();
    particles.boat_wake(Vec3::new(8.0, 64.0, 8.0), 90.0, 0.10, Vec3::ZERO);
    assert_eq!(particles.active_count(), 0);
    // 1 + 0.3 * 60 = 19 splashes at a top speed of 0.3.
    particles.boat_wake(
        Vec3::new(8.0, 64.0, 8.0),
        90.0,
        0.30,
        Vec3::new(0.3, 0.0, 0.0),
    );
    assert_eq!(particles.active_count(), 19);
}

#[test]
fn slime_wolf_and_spawner_bursts_match_beta_counts() {
    let feet = Vec3::new(8.0, 65.0, 8.0);
    let mut particles = EffectParticles::default();
    particles.slime_splat(feet, 2);
    assert_eq!(particles.active_count(), 16);
    let mut particles = EffectParticles::default();
    particles.spawn_puffs(feet, 0.6, 1.8);
    assert_eq!(particles.active_count(), 20);
    let mut particles = EffectParticles::default();
    // The spray starts after 0.4 and peaks at seven drops, 0.9 into the shake.
    particles.wolf_spray(feet, 0.6, 0.3, Vec3::ZERO);
    assert_eq!(particles.active_count(), 0);
    particles.wolf_spray(feet, 0.6, 0.9, Vec3::ZERO);
    assert_eq!(particles.active_count(), 7);
}
