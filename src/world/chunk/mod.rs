//! Chunk coordinates, compact block and metadata storage, heightmaps, and
//! the loaded [`WorldChunks`] collection.
//!
//! Column climate lives in [`crate::world::biome`], independent of which
//! generator filled the chunk.

mod chunk;
mod neighbors;
mod position;

pub use chunk::CHUNK_HEIGHT;
pub use chunk::CHUNK_SIZE;
pub use chunk::ChestGroup;
pub use chunk::Chunk;
pub use chunk::NoteState;
pub use chunk::OpenCart;
pub use chunk::PendingTick;
pub use chunk::SECTION_HEIGHT;
pub use chunk::SECTIONS_PER_CHUNK;
pub use chunk::SignText;
pub use chunk::WorldChunks;
pub use chunk::remesh_chunks_touching;
pub use neighbors::ChunkNeighbors;
pub use position::ChunkPosition;

mod generated;
mod heightmap;

pub use generated::ChunkDroppedItem;
pub use generated::GeneratedChunk;
pub use heightmap::Heightmap;
