use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::Mesh,
    render::render_resource::PrimitiveTopology,
};

use crate::world::{
    block::block::BlockId,
    chunk::{CHUNK_HEIGHT, CHUNK_SIZE, Chunk},
    lighting::Skylight,
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
                    let base = block_color(block, face_index == 0);
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
    .with_inserted_indices(Indices::U32(indices))
}

fn block_color(block: BlockId, top: bool) -> [f32; 3] {
    match block {
        BlockId::Grass if top => [0.32, 0.68, 0.22],
        BlockId::Grass | BlockId::Dirt => [0.48, 0.32, 0.18],
        BlockId::Stone => [0.54, 0.54, 0.54],
        _ => [0.8, 0.8, 0.8],
    }
}
