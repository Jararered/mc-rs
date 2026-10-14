//! `RenderPainting`.
//!
//! A painting is drawn one block-sized tile at a time from `art/kz.png`, so
//! each tile can take the light of the cell in front of it. A tile is a box
//! one pixel deep: the motive's square of the sheet on the front, and the
//! sheet's wooden back and frame on the other five sides. The painting's own
//! `Transform` already carries its yaw, so the model only scales pixels to
//! blocks.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::mesh::MeshTag;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use super::CreatureAssets;
use super::CreatureMaterial;
use super::cell_brightness;
use super::creature_tag;
use crate::entity::painting::Art;
use crate::entity::painting::Painting;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
use crate::world::tick::WorldTick;

pub(super) const TEXTURE: &str = "art/kz.png";

/// One tile of a painting's model: its column from the model's −X side and
/// its row from the bottom.
#[derive(Component)]
pub(super) struct PaintingTile {
    owner: Entity,
    column: i32,
    row: i32,
}

/// `func_159_a`'s box for one tile, in model pixels: the tile's centre is at
/// `(x, y)` and the box runs from −0.5 to 0.5 in Z, the front facing −Z.
pub fn tile_mesh(art: Art, column: i32, row: i32) -> Mesh {
    let motive = art.motive();
    let (width, height) = (f32::from(motive.size.0), f32::from(motive.size.1));
    let (offset_x, offset_y) = (f32::from(motive.offset.0), f32::from(motive.offset.1));
    let (column, row) = (column as f32, row as f32);
    let left = -width / 2.0;
    let bottom = -height / 2.0;
    let x1 = left + (column + 1.0) * 16.0;
    let x0 = left + column * 16.0;
    let y1 = bottom + (row + 1.0) * 16.0;
    let y0 = bottom + row * 16.0;
    let (front, back) = (-0.5, 0.5);
    let u0 = (offset_x + width - column * 16.0) / 256.0;
    let u1 = (offset_x + width - (column + 1.0) * 16.0) / 256.0;
    let v0 = (offset_y + height - row * 16.0) / 256.0;
    let v1 = (offset_y + height - (row + 1.0) * 16.0) / 256.0;
    // The back, the frame's top and bottom, and its sides, as `func_159_a`
    // names them.
    let (back_u0, back_u1, back_v0, back_v1) = (0.75, 0.8125, 0.0, 0.0625);
    let (rim_u0, rim_u1, rim_v) = (0.75, 0.8125, 0.001_953_125);
    let (side_u, side_v0, side_v1) = (0.751_953_1, 0.0, 0.0625);
    let quads: [([f32; 3], [([f32; 3], [f32; 2]); 4]); 6] = [
        (
            [0.0, 0.0, -1.0],
            [
                ([x1, y0, front], [u1, v0]),
                ([x0, y0, front], [u0, v0]),
                ([x0, y1, front], [u0, v1]),
                ([x1, y1, front], [u1, v1]),
            ],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                ([x1, y1, back], [back_u0, back_v0]),
                ([x0, y1, back], [back_u1, back_v0]),
                ([x0, y0, back], [back_u1, back_v1]),
                ([x1, y0, back], [back_u0, back_v1]),
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                ([x1, y1, front], [rim_u0, rim_v]),
                ([x0, y1, front], [rim_u1, rim_v]),
                ([x0, y1, back], [rim_u1, rim_v]),
                ([x1, y1, back], [rim_u0, rim_v]),
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                ([x1, y0, back], [rim_u0, rim_v]),
                ([x0, y0, back], [rim_u1, rim_v]),
                ([x0, y0, front], [rim_u1, rim_v]),
                ([x1, y0, front], [rim_u0, rim_v]),
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                ([x1, y1, back], [side_u, side_v0]),
                ([x1, y0, back], [side_u, side_v1]),
                ([x1, y0, front], [side_u, side_v1]),
                ([x1, y1, front], [side_u, side_v0]),
            ],
        ),
        (
            [1.0, 0.0, 0.0],
            [
                ([x0, y1, front], [side_u, side_v0]),
                ([x0, y0, front], [side_u, side_v1]),
                ([x0, y0, back], [side_u, side_v1]),
                ([x0, y1, back], [side_u, side_v0]),
            ],
        ),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in quads {
        let base = positions.len() as u16;
        for (position, uv) in corners {
            positions.push(position);
            normals.push(normal);
            uvs.push(uv);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U16(indices))
}

/// `func_160_a`: the cell whose light a tile takes, in front of the wall.
pub fn tile_light_cell(painting: &Painting, center: Vec3, column: i32, row: i32) -> IVec3 {
    let motive = painting.art.motive();
    let tile_x = -f32::from(motive.size.0) / 2.0 + column as f32 * 16.0 + 8.0;
    let tile_y = -f32::from(motive.size.1) / 2.0 + row as f32 * 16.0 + 8.0;
    let mut cell = Vec3::new(center.x, center.y + tile_y / 16.0, center.z);
    match painting.direction {
        0 => cell.x = center.x + tile_x / 16.0,
        1 => cell.z = center.z - tile_x / 16.0,
        2 => cell.x = center.x - tile_x / 16.0,
        _ => cell.z = center.z + tile_x / 16.0,
    }
    cell.floor().as_ivec3()
}

pub(super) fn add_painting_models(
    mut commands: Commands,
    assets: Res<CreatureAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tiles: Local<HashMap<(Art, i32, i32), Handle<Mesh>>>,
    paintings: Query<(Entity, &Painting), Added<Painting>>,
) {
    if paintings.is_empty() {
        return;
    }
    let Some(skin) = assets.skins.get(TEXTURE) else {
        return;
    };
    for (entity, painting) in &paintings {
        let (columns, rows) = painting.art.motive().blocks();
        let model = commands
            .spawn((
                Name::new("Painting model"),
                Transform::from_scale(Vec3::splat(1.0 / 16.0)),
                Visibility::Inherited,
                ChildOf(entity),
            ))
            .id();
        for column in 0..columns {
            for row in 0..rows {
                let mesh = tiles
                    .entry((painting.art, column, row))
                    .or_insert_with(|| meshes.add(tile_mesh(painting.art, column, row)))
                    .clone();
                commands.spawn((
                    PaintingTile {
                        owner: entity,
                        column,
                        row,
                    },
                    Mesh3d(mesh),
                    MeshMaterial3d::<CreatureMaterial>(skin.clone()),
                    MeshTag::default(),
                    Transform::default(),
                    Visibility::Inherited,
                    ChildOf(model),
                ));
            }
        }
    }
}

/// Each tile takes the brightness of its own cell.
pub(super) fn pose_paintings(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    environment: crate::world::dimension::Environment,
    paintings: Query<(&Painting, &Transform)>,
    mut tiles: Query<(&PaintingTile, &mut MeshTag)>,
) {
    let partial = tick.partial();
    let subtracted = environment.skylight_subtracted(partial);
    let ambient = environment.ambient_light();
    for (tile, mut tag) in &mut tiles {
        let Ok((painting, transform)) = paintings.get(tile.owner) else {
            continue;
        };
        let cell = tile_light_cell(painting, transform.translation, tile.column, tile.row);
        let brightness = cell_brightness(&chunks, light.as_deref(), cell, subtracted, ambient);
        tag.set_if_neq(creature_tag(brightness, false, 0.0, 1.0));
    }
}
