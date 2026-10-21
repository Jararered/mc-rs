//! `TileEntityRendererPiston`: the blocks a piston is moving, drawn sliding
//! between ticks. The chunk mesher leaves `MovingPiston` cells empty.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::rendering::meshing::MOVING_CELL;
use crate::rendering::meshing::moving_block_meshes;
use crate::rendering::textures::AlphaMaskMaterial;
use crate::rendering::textures::CutoutMaterial;
use crate::rendering::textures::GrassOverlayMaterial;
use crate::rendering::textures::TerrainMaterial;
use crate::rendering::textures::WaterMaterial;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
use crate::world::lighting::column_channels;
use crate::world::moving_piston::MovingPiston;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

/// A landed block is kept on screen at most this many frames while its
/// chunk is remeshed.
const MAX_LANDED_FRAMES: u32 = 60;

/// One block to draw: `(block, metadata, short_arm)`, and whether it slides.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Piece {
    block: Block,
    metadata: u8,
    short_arm: bool,
    slides: bool,
}

#[derive(Component)]
pub(crate) struct MovingPistonVisual {
    position: IVec3,
    sequence: u64,
    pieces: Vec<Piece>,
    /// Frames since the move ended, while the chunk catches up.
    landed: Option<u32>,
}

/// A child mesh of a [`MovingPistonVisual`] that follows the slide.
#[derive(Component)]
pub(crate) struct SlidingPiece;

/// What `TileEntityRendererPiston` draws for a move at `progress`.
fn pieces(moving: &MovingPiston, progress: f32) -> Vec<Piece> {
    let piece = |block, metadata, short_arm, slides| Piece {
        block,
        metadata,
        short_arm,
        slides,
    };
    if moving.block == Block::PistonHead && progress < 0.5 {
        vec![piece(moving.block, moving.metadata, true, true)]
    } else if moving.source && !moving.extending {
        let sticky = if moving.block == Block::StickyPiston {
            8
        } else {
            0
        };
        vec![
            piece(
                Block::PistonHead,
                moving.facing | sticky,
                progress >= 0.5,
                true,
            ),
            // `func_31078_d`: the base keeps its extended shape.
            piece(moving.block, moving.facing | 8, false, false),
        ]
    } else {
        vec![piece(moving.block, moving.metadata, false, true)]
    }
}

/// The light a moving block is drawn with: the brightest of its cell and the
/// six around it, per channel, so a block leaving a solid cell is not black.
fn light_around(chunks: &WorldChunks, light: Option<&LightCache>, cell: IVec3) -> (u8, u8) {
    let mut brightest = (0, 0);
    for offset in [
        IVec3::ZERO,
        IVec3::X,
        IVec3::NEG_X,
        IVec3::Y,
        IVec3::NEG_Y,
        IVec3::Z,
        IVec3::NEG_Z,
    ] {
        let at = cell + offset;
        let (sky, block) = light
            .and_then(|light| light.channels(at.x, at.y, at.z))
            .unwrap_or_else(|| column_channels(chunks, at.x, at.y, at.z));
        brightest = (brightest.0.max(sky), brightest.1.max(block));
    }
    brightest
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MovingMaterials<'w> {
    terrain: Option<Res<'w, TerrainMaterial>>,
    grass_overlay: Option<Res<'w, GrassOverlayMaterial>>,
    cutout: Option<Res<'w, CutoutMaterial>>,
    water: Option<Res<'w, WaterMaterial>>,
    alpha_mask: Option<Res<'w, AlphaMaskMaterial>>,
}

pub(crate) type MeshKey = (Piece, (u8, u8), bool);

/// Spawn, slide, and retire the visuals of moving blocks.
pub(crate) fn sync_moving_piston_rendering(
    mut commands: Commands,
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    streaming: Option<Res<WorldStreaming>>,
    settings: Option<Res<GameSettings>>,
    materials: MovingMaterials,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: Local<HashMap<MeshKey, Vec<(usize, Handle<Mesh>)>>>,
    mut visuals: Query<(Entity, &mut MovingPistonVisual, Option<&Children>)>,
    mut sliding: Query<&mut Transform, With<SlidingPiece>>,
) {
    let (Some(terrain), Some(grass_overlay), Some(cutout), Some(water), Some(alpha_mask)) = (
        materials.terrain,
        materials.grass_overlay,
        materials.cutout,
        materials.water,
        materials.alpha_mask,
    ) else {
        return;
    };
    let layer_materials = [
        &terrain.0,
        &grass_overlay.0,
        &cutout.0,
        &water.0,
        &alpha_mask.0,
    ];
    let fancy = settings.is_some_and(|settings| settings.graphics.fancy_leaves());
    let partial = tick.partial();
    let moving: HashMap<(IVec3, u64), MovingPiston> = chunks
        .moving_pistons()
        .into_iter()
        .map(|(position, moving)| ((position, moving.sequence), moving))
        .collect();
    if moving.is_empty() && visuals.is_empty() {
        cache.clear();
        return;
    }
    let center = -Vec3::new(MOVING_CELL[0] as f32, 0.0, MOVING_CELL[2] as f32);
    let mut spawn_pieces = |commands: &mut Commands,
                            parent: Entity,
                            position: IVec3,
                            pieces: &[Piece],
                            offset: Vec3| {
        let lit = light_around(&chunks, light.as_deref(), position);
        for &piece in pieces {
            let layers = cache.entry((piece, lit, fancy)).or_insert_with(|| {
                moving_block_meshes(piece.block, piece.metadata, lit, fancy, piece.short_arm)
                    .into_layers()
                    .into_iter()
                    .enumerate()
                    .filter(|(_, layer)| !layer.is_empty())
                    .map(|(index, layer)| (index, meshes.add(layer.into_mesh())))
                    .collect()
            });
            for (layer, mesh) in layers.iter() {
                let translation = center + if piece.slides { offset } else { Vec3::ZERO };
                let mut child = commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(layer_materials[*layer].clone()),
                    Transform::from_translation(translation),
                    NoFrustumCulling,
                ));
                if piece.slides {
                    child.insert(SlidingPiece);
                }
                let child = child.id();
                commands.entity(parent).add_child(child);
            }
        }
    };

    let mut shown = HashMap::new();
    for (entity, mut visual, children) in &mut visuals {
        let key = (visual.position, visual.sequence);
        if let Some(state) = moving.get(&key) {
            shown.insert(key, ());
            let progress = state.progress_at(partial);
            let wanted = pieces(state, progress);
            let offset = state.offset(partial);
            if wanted != visual.pieces {
                commands.entity(entity).despawn_children();
                spawn_pieces(&mut commands, entity, visual.position, &wanted, offset);
                visual.pieces = wanted;
            } else if let Some(children) = children {
                for child in children.iter() {
                    if let Ok(mut transform) = sliding.get_mut(child) {
                        transform.translation = center + offset;
                    }
                }
            }
            continue;
        }
        // The move ended. Show the block as it landed until its chunk's new
        // mesh is in.
        let frames = visual.landed.map_or(0, |frames| frames + 1);
        let chunk = ChunkPosition::from_block(visual.position.x, visual.position.z);
        let pending = streaming
            .as_deref()
            .is_some_and(|streaming| streaming.remesh_pending(chunk));
        if visual.landed.is_some() && (!pending || frames > MAX_LANDED_FRAMES) {
            commands.entity(entity).despawn();
            continue;
        }
        if visual.landed.is_none() {
            let block = chunks.block_at(visual.position.x, visual.position.y, visual.position.z);
            let landed = block
                .filter(|block| !matches!(block, Block::Air | Block::MovingPiston))
                .map(|block| Piece {
                    block,
                    metadata: chunks.metadata_at(
                        visual.position.x,
                        visual.position.y,
                        visual.position.z,
                    ),
                    short_arm: false,
                    slides: false,
                });
            commands.entity(entity).despawn_children();
            let landed: Vec<Piece> = landed.into_iter().collect();
            spawn_pieces(&mut commands, entity, visual.position, &landed, Vec3::ZERO);
            visual.pieces = landed;
        }
        visual.landed = Some(frames);
    }
    for (&(position, sequence), state) in &moving {
        if shown.contains_key(&(position, sequence)) {
            continue;
        }
        let wanted = pieces(state, state.progress_at(partial));
        let entity = commands
            .spawn((
                Name::new("Moving block"),
                Transform::from_translation(position.as_vec3()),
                Visibility::default(),
            ))
            .id();
        spawn_pieces(
            &mut commands,
            entity,
            position,
            &wanted,
            state.offset(partial),
        );
        commands.entity(entity).insert(MovingPistonVisual {
            position,
            sequence,
            pieces: wanted,
            landed: None,
        });
    }
}
