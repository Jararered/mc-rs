use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use game::app::state::AppScreen;
use game::block::id::BlockId;
use game::entity::particles::block::BlockParticlePlugin;
use game::entity::particles::block::BlockParticles;
use game::entity::particles::registry::ParticleSprite;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::world::chunk::Chunk;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;
use game::world::textures::FoliageColors;
use game::world::textures::GrassColors;
use game::world::textures::PALETTE_SIZE;
use game::world::textures::atlas_tile_uvs;
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
        block: BlockId::Stone,
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
        game::world::chunk::ChunkPos::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            chunk,
            biomes: BiomeMap::from_cells([climate; 16 * 16]),
            items: Vec::new(),
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
    for block in [BlockId::TallGrass, BlockId::Fern] {
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
                    .emit_break(hit);
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
            block: BlockId::Leaves,
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
            block: BlockId::TallGrass,
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
