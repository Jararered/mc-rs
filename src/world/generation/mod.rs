use crate::world::{
    block::block::BlockId,
    chunk::{CHUNK_HEIGHT, CHUNK_SIZE, Chunk, ChunkPos},
};

/// A small deterministic terrain pass for the first visible chunk.
pub fn generate_chunk(position: ChunkPos) -> Chunk {
    let mut chunk = Chunk::new();

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let world_x = position.x * CHUNK_SIZE as i32 + x as i32;
            let world_z = position.z * CHUNK_SIZE as i32 + z as i32;
            let height = terrain_height(world_x, world_z);

            for y in 0..=height {
                let block = if y == height {
                    BlockId::Grass
                } else if y + 3 >= height {
                    BlockId::Dirt
                } else {
                    BlockId::Stone
                };
                chunk.set(x, y, z, block);
            }
        }
    }

    chunk
}

fn terrain_height(x: i32, z: i32) -> usize {
    let x = x as f32;
    let z = z as f32;
    let height = 6.0 + (x * 0.42).sin() * 1.8 + (z * 0.32).cos() * 1.5;
    (height.round() as usize).clamp(2, CHUNK_HEIGHT - 2)
}
