use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::Mesh,
    render::render_resource::PrimitiveTopology,
};

use crate::world::{
    block::block::BlockId,
    chunk::{CHUNK_HEIGHT, CHUNK_SIZE, Chunk},
    lighting::Skylight,
    textures::block_tile,
};

struct Face {
    neighbor: [i32; 3],
    normal: [f32; 3],
    corners: [[f32; 3]; 4],
    shade: f32,
}

const FACES: [Face; 6] = [
    Face {
        neighbor: [0, 1, 0],
        normal: [0.0, 1.0, 0.0],
        corners: [
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ],
        shade: 1.0,
    },
    Face {
        neighbor: [0, -1, 0],
        normal: [0.0, -1.0, 0.0],
        corners: [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        shade: 0.55,
    },
    Face {
        neighbor: [1, 0, 0],
        normal: [1.0, 0.0, 0.0],
        corners: [
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
        ],
        shade: 0.8,
    },
    Face {
        neighbor: [-1, 0, 0],
        normal: [-1.0, 0.0, 0.0],
        corners: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
        shade: 0.8,
    },
    Face {
        neighbor: [0, 0, 1],
        normal: [0.0, 0.0, 1.0],
        corners: [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        shade: 0.9,
    },
    Face {
        neighbor: [0, 0, -1],
        normal: [0.0, 0.0, -1.0],
        corners: [
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
        shade: 0.9,
    },
];

/// Emit only faces touching air. A missing neighbor is treated as air for this isolated chunk.
pub fn mesh_chunk(chunk: &Chunk, skylight: &Skylight) -> Mesh {
    let mut positions = Vec::<[f32; 3]>::new();
    let mut normals = Vec::<[f32; 3]>::new();
    let mut colors = Vec::<[f32; 4]>::new();
    let mut uvs = Vec::<[f32; 2]>::new();
    let mut indices = Vec::<u32>::new();

    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.get(x, y, z).unwrap();
                if block == BlockId::Air {
                    continue;
                }

                for (face_index, face) in FACES.iter().enumerate() {
                    let nx = x as i32 + face.neighbor[0];
                    let ny = y as i32 + face.neighbor[1];
                    let nz = z as i32 + face.neighbor[2];
                    let neighbor = (nx >= 0 && ny >= 0 && nz >= 0)
                        .then(|| chunk.get(nx as usize, ny as usize, nz as usize))
                        .flatten();
                    if neighbor.is_some_and(|block| block != BlockId::Air) {
                        continue;
                    }

                    let level = (nx >= 0 && ny >= 0 && nz >= 0)
                        .then(|| skylight.get(nx as usize, ny as usize, nz as usize))
                        .flatten()
                        .unwrap_or(15);
                    let brightness = (0.35 + 0.65 * level as f32 / 15.0) * face.shade;
                    let base = block_tint(block, face_index == 0);
                    let color = [
                        base[0] * brightness,
                        base[1] * brightness,
                        base[2] * brightness,
                        1.0,
                    ];
                    let start = positions.len() as u32;

                    for corner in face.corners {
                        positions.push([
                            x as f32 + corner[0],
                            y as f32 + corner[1],
                            z as f32 + corner[2],
                        ]);
                        normals.push(face.normal);
                        colors.push(color);
                    }
                    uvs.extend_from_slice(&face_uvs(block, face_index));
                    indices.extend_from_slice(&[
                        start,
                        start + 1,
                        start + 2,
                        start,
                        start + 2,
                        start + 3,
                    ]);
                }
            }
        }
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

fn face_uvs(block: BlockId, face: usize) -> [[f32; 2]; 4] {
    let (tile_x, tile_y) = block_tile(block, face);
    // Stay half a texel inside the tile to keep adjacent atlas tiles from bleeding.
    const INSET: f32 = 0.5 / 256.0;
    let u0 = tile_x as f32 / 16.0 + INSET;
    let v0 = tile_y as f32 / 16.0 + INSET;
    let u1 = (tile_x as f32 + 1.0) / 16.0 - INSET;
    let v1 = (tile_y as f32 + 1.0) / 16.0 - INSET;

    match face {
        0 | 1 => [[u0, v0], [u0, v1], [u1, v1], [u1, v0]],
        2 | 5 => [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        _ => [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
    }
}

fn block_tint(block: BlockId, top: bool) -> [f32; 3] {
    match block {
        BlockId::Grass if top => [0.55, 0.8, 0.4],
        BlockId::Water => [0.4, 0.6, 0.95],
        _ => [1.0, 1.0, 1.0],
    }
}
