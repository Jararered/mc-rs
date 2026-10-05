//! The native world format: one JSON file per chunk, grouped into region
//! folders, next to `level.json` and `player.json`.
//!
//! ```text
//! level.json          world manifest: name, seed, creation time
//! player.json         camera pose and inventory
//! regions/
//!   region0,0/        chunks 0..15 x 0..15
//!     chunk0,0.bin
//!     chunk1,0.bin
//!   region0,-1/       chunks 0..15 x -16..-1
//!     chunk0,-1.bin
//! ```
//!
//! Each chunk is written through a temporary path so an interrupted save
//! cannot corrupt the previous one. This format keeps everything the game
//! simulates (biome climate, pending block ticks, mob state), which the Beta
//! format in [`super::original`] has no room for.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use super::StoredStack;
use crate::block::blocks::Block;
use crate::world::biome::Biome;
use crate::world::biome::BiomeMap;
use crate::world::biome::Climate;
use crate::world::chest::CHEST_SLOTS;
use crate::world::chest::Chest;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkDroppedItem;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::Heightmap;
use crate::world::chunk::PendingTick;
use crate::world::furnace::FURNACE_SLOTS;
use crate::world::furnace::Furnace;

/// Chunks per region along each axis. `region0,0` covers chunks 0..15.
pub const REGION_SIZE: i32 = 16;
/// Chunk file version. Version 2 stores species and orientation in block
/// metadata instead of in extra block ids; version 1 chunks regenerate.
pub const CHUNK_FORMAT_VERSION: u32 = 2;

const BLOCKS_PER_CHUNK: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;
const COLUMNS_PER_CHUNK: usize = CHUNK_SIZE * CHUNK_SIZE;

/// The region a chunk belongs to, as `(region_x, region_z)`.
pub fn region_of(position: ChunkPosition) -> (i32, i32) {
    (
        position.x.div_euclid(REGION_SIZE),
        position.z.div_euclid(REGION_SIZE),
    )
}

/// Folder inside a world folder that holds every region folder.
pub const REGIONS_DIRECTORY: &str = "regions";

/// Folder name for a region inside [`REGIONS_DIRECTORY`], for example `region0,0`.
pub fn region_dir_name(region: (i32, i32)) -> String {
    format!("region{},{}", region.0, region.1)
}

/// Move region folders from the old layout, `regionsX,Z` directly in the world
/// folder, into `regions/regionX,Z`. A folder whose new name already exists is
/// left where it is.
pub(super) fn migrate_region_folders(root: &Path) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(coordinates) = name
            .to_str()
            .and_then(|name| name.strip_prefix("regions"))
            .filter(|rest| !rest.is_empty() && rest.contains(','))
        else {
            continue;
        };
        if !entry.path().is_dir() {
            continue;
        }
        let target = root
            .join(REGIONS_DIRECTORY)
            .join(format!("region{coordinates}"));
        if target.exists() {
            continue;
        }
        fs::create_dir_all(root.join(REGIONS_DIRECTORY))?;
        fs::rename(entry.path(), target)?;
    }
    Ok(())
}

/// File name for a chunk, for example `chunk0,0.bin`.
pub fn chunk_file_name(position: ChunkPosition) -> String {
    format!("chunk{},{}.bin", position.x, position.z)
}

/// The on-disk form of a chunk. Blocks are run-length encoded because generated
/// terrain is mostly long runs of air and stone.
#[derive(Serialize, Deserialize)]
pub(super) struct StoredChunk {
    format_version: u32,
    runs: Vec<(u8, u16)>,
    heightmap: Vec<u8>,
    biomes: Vec<StoredClimate>,
    /// Absent on chunks saved before dropped items were stored.
    #[serde(default)]
    items: Vec<StoredDroppedItem>,
    #[serde(default)]
    mobs: Vec<crate::entity::mobs::MobRecord>,
    #[serde(default)]
    spawners: Vec<(u16, crate::entity::mobs::MobSpawner)>,
    /// Absent on chunks saved before furnace inventories were added.
    #[serde(default)]
    furnaces: Vec<StoredFurnace>,
    /// Absent on chunks saved before chest inventories were added.
    #[serde(default)]
    chests: Vec<StoredChest>,
    /// Absent on chunks saved before population ran across chunks. Those were
    /// decorated in full when generated.
    #[serde(default = "populated_default")]
    populated: bool,
    /// Run-length encoded metadata nibbles, two blocks per byte in chunk
    /// index order. Empty when every value is zero, and absent on chunks
    /// saved before block metadata existed.
    #[serde(default)]
    metadata: Vec<(u8, u16)>,
    /// Scheduled block ticks that were pending when the chunk was unloaded.
    /// Absent on chunks saved before block ticks existed.
    #[serde(default)]
    ticks: Vec<StoredTick>,
}

/// A pending scheduled tick, stored relative to the save so a chunk that
/// stays unloaded for a while resumes with the same remaining delay.
#[derive(Serialize, Deserialize)]
struct StoredTick {
    index: u16,
    block: u8,
    delay: u32,
}

const fn populated_default() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct StoredFurnace {
    index: u16,
    slots: [Option<StoredStack>; FURNACE_SLOTS],
    burn_ticks: u16,
    fuel_ticks: u16,
    cook_ticks: u16,
}

#[derive(Serialize, Deserialize)]
struct StoredChest {
    index: u16,
    slots: [Option<StoredStack>; CHEST_SLOTS],
}

#[derive(Serialize, Deserialize)]
struct StoredDroppedItem {
    stack: StoredStack,
    x: f32,
    y: f32,
    z: f32,
    motion_x: f32,
    motion_y: f32,
    motion_z: f32,
    age_ticks: u32,
    pickup_delay_ticks: u16,
    hover_start: f32,
    rng_state: u64,
}

/// Climate quantized to a byte per field. Temperature and humidity only feed the
/// 256x256 grass/foliage palette lookup, so a byte is visually lossless there.
#[derive(Serialize, Deserialize)]
struct StoredClimate {
    temperature: u8,
    humidity: u8,
    biome: u8,
}

impl StoredChunk {
    /// Snapshot a live chunk. `items` and `ticks` carry the dropped items and
    /// pending block ticks that live outside the chunk while it is loaded, so
    /// they are passed in rather than written into the chunk and read back.
    pub(super) fn from_generated(
        generated: &GeneratedChunk,
        items: &[ChunkDroppedItem],
        ticks: &[PendingTick],
    ) -> Self {
        Self {
            format_version: CHUNK_FORMAT_VERSION,
            runs: encode_blocks(generated.chunk.raw_blocks()),
            heightmap: generated.heightmap.heights().to_vec(),
            biomes: generated
                .biomes
                .cells()
                .iter()
                .map(|climate| StoredClimate {
                    temperature: quantize(climate.temperature),
                    humidity: quantize(climate.humidity),
                    biome: climate.biome.as_u8(),
                })
                .collect(),
            items: items
                .iter()
                .map(|item| StoredDroppedItem {
                    stack: StoredStack::from_stack(item.stack),
                    x: item.position[0],
                    y: item.position[1],
                    z: item.position[2],
                    motion_x: item.motion[0],
                    motion_y: item.motion[1],
                    motion_z: item.motion[2],
                    age_ticks: item.age_ticks,
                    pickup_delay_ticks: item.pickup_delay_ticks,
                    hover_start: item.hover_start,
                    rng_state: item.rng_state,
                })
                .collect(),
            mobs: generated.chunk.mob_records().to_vec(),
            spawners: generated
                .chunk
                .spawners()
                .map(|(index, spawner)| (index as u16, *spawner))
                .collect(),
            furnaces: generated
                .chunk
                .furnaces()
                .map(|(index, furnace)| StoredFurnace {
                    index: index as u16,
                    slots: furnace
                        .slots
                        .map(|stack| stack.map(StoredStack::from_stack)),
                    burn_ticks: furnace.burn_ticks,
                    fuel_ticks: furnace.fuel_ticks,
                    cook_ticks: furnace.cook_ticks,
                })
                .collect(),
            chests: generated
                .chunk
                .chests()
                .map(|(index, chest)| StoredChest {
                    index: index as u16,
                    slots: chest.slots.map(|stack| stack.map(StoredStack::from_stack)),
                })
                .collect(),
            populated: generated.populated,
            metadata: generated
                .chunk
                .raw_metadata()
                .map(encode_blocks)
                .unwrap_or_default(),
            ticks: ticks
                .iter()
                .map(|tick| StoredTick {
                    index: tick.index,
                    block: tick.block.as_u8(),
                    delay: tick.delay,
                })
                .collect(),
        }
    }

    /// The chunk as the bytes of its file. Encoding is the expensive half of a
    /// save, which is why it belongs off the main thread.
    pub(super) fn to_bytes(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(io::Error::other)
    }

    pub(super) fn into_generated(self) -> Option<GeneratedChunk> {
        if self.format_version != CHUNK_FORMAT_VERSION {
            return None;
        }
        if self.heightmap.len() != COLUMNS_PER_CHUNK || self.biomes.len() != COLUMNS_PER_CHUNK {
            return None;
        }
        let blocks = decode_blocks(&self.runs)?;

        let mut heights = [0u8; COLUMNS_PER_CHUNK];
        heights.copy_from_slice(&self.heightmap);

        let cells: [Climate; COLUMNS_PER_CHUNK] = std::array::from_fn(|index| {
            let stored = &self.biomes[index];
            Climate {
                temperature: dequantize(stored.temperature),
                humidity: dequantize(stored.humidity),
                biome: Biome::from_u8(stored.biome).unwrap_or(Biome::Plains),
            }
        });

        let mut chunk = Chunk::from_blocks(blocks);
        chunk.set_mob_records(self.mobs);
        for (index, spawner) in self.spawners {
            chunk.insert_spawner(usize::from(index), spawner);
        }
        if !self.metadata.is_empty() {
            chunk.set_raw_metadata(decode_runs(&self.metadata, BLOCKS_PER_CHUNK / 2)?);
        }
        chunk.set_pending_ticks(
            self.ticks
                .into_iter()
                .filter(|tick| usize::from(tick.index) < BLOCKS_PER_CHUNK)
                .filter_map(|tick| {
                    Some(PendingTick {
                        index: tick.index,
                        block: Block::from_u8(tick.block)?,
                        delay: tick.delay,
                    })
                })
                .collect(),
        );
        for furnace in self.furnaces {
            let index = usize::from(furnace.index);
            if index >= BLOCKS_PER_CHUNK {
                continue;
            }
            let y = index / (CHUNK_SIZE * CHUNK_SIZE);
            let z = index / CHUNK_SIZE % CHUNK_SIZE;
            let x = index % CHUNK_SIZE;
            if !matches!(
                chunk.get(x, y, z),
                Some(block) if block.is_furnace()
            ) {
                continue;
            }
            let slots = furnace
                .slots
                .map(|stack| stack.and_then(StoredStack::into_stack));
            chunk.insert_furnace(
                index,
                Furnace {
                    slots,
                    burn_ticks: furnace.burn_ticks,
                    fuel_ticks: furnace.fuel_ticks,
                    cook_ticks: furnace
                        .cook_ticks
                        .min(crate::world::furnace::SMELT_TICKS - 1),
                },
            );
        }
        for chest in self.chests {
            let index = usize::from(chest.index);
            if index >= BLOCKS_PER_CHUNK {
                continue;
            }
            let y = index / (CHUNK_SIZE * CHUNK_SIZE);
            let z = index / CHUNK_SIZE % CHUNK_SIZE;
            let x = index % CHUNK_SIZE;
            if !chunk.get(x, y, z).is_some_and(Block::is_chest) {
                continue;
            }
            let slots = chest
                .slots
                .map(|stack| stack.and_then(StoredStack::into_stack));
            chunk.insert_chest(index, Chest { slots });
        }

        Some(GeneratedChunk {
            chunk,
            heightmap: Heightmap::from_heights(heights),
            biomes: BiomeMap::from_cells(cells),
            items: self
                .items
                .into_iter()
                .filter_map(|item| {
                    item.stack.into_stack().map(|stack| ChunkDroppedItem {
                        stack,
                        position: [item.x, item.y, item.z],
                        motion: [item.motion_x, item.motion_y, item.motion_z],
                        age_ticks: item.age_ticks,
                        pickup_delay_ticks: item.pickup_delay_ticks,
                        hover_start: item.hover_start,
                        rng_state: item.rng_state,
                    })
                })
                .collect(),
            populated: self.populated,
        })
    }
}

fn decode_runs(runs: &[(u8, u16)], expected: usize) -> Option<Vec<u8>> {
    let total: usize = runs.iter().map(|(_, length)| *length as usize).sum();
    if total != expected {
        return None;
    }
    let mut bytes = Vec::with_capacity(total);
    for (value, length) in runs {
        bytes.resize(bytes.len() + *length as usize, *value);
    }
    Some(bytes)
}

fn encode_blocks(blocks: &[u8]) -> Vec<(u8, u16)> {
    let mut runs = Vec::new();
    let Some((&first, rest)) = blocks.split_first() else {
        return runs;
    };
    let mut value = first;
    let mut length: u16 = 1;
    for &next in rest {
        if next == value && length < u16::MAX {
            length += 1;
        } else {
            runs.push((value, length));
            value = next;
            length = 1;
        }
    }
    runs.push((value, length));
    runs
}

fn decode_blocks(runs: &[(u8, u16)]) -> Option<Vec<Block>> {
    let total: usize = runs.iter().map(|(_, length)| *length as usize).sum();
    if total != BLOCKS_PER_CHUNK {
        return None;
    }
    let mut blocks = Vec::with_capacity(total);
    for (value, length) in runs {
        let block = Block::from_u8(*value)?;
        blocks.resize(blocks.len() + *length as usize, block);
    }
    Some(blocks)
}

fn quantize(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn dequantize(value: u8) -> f64 {
    f64::from(value) / 255.0
}

fn write_chunk_file(path: &Path, chunk: &StoredChunk) -> io::Result<()> {
    let bytes = chunk.to_bytes()?;
    // Write through a temporary file so an interrupted save leaves the previous
    // chunk intact instead of a half-written one.
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

fn region_path(root: &Path, region: (i32, i32)) -> PathBuf {
    root.join(REGIONS_DIRECTORY).join(region_dir_name(region))
}

fn chunk_path(root: &Path, position: ChunkPosition) -> PathBuf {
    region_path(root, region_of(position)).join(chunk_file_name(position))
}

/// Load a stored chunk, or `None` if it was never saved or cannot be read.
pub(super) fn load_chunk(root: &Path, position: ChunkPosition) -> Option<GeneratedChunk> {
    let path = chunk_path(root, position);
    match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<StoredChunk>(&bytes) {
            Ok(stored) => stored.into_generated(),
            Err(error) => {
                warn!("Ignoring unreadable chunk {}: {error}", path.display());
                None
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            warn!("Ignoring unreadable chunk {}: {error}", path.display());
            None
        }
    }
}

/// Write chunks that are already in their stored form, creating each region
/// folder as needed. Returns how many chunks were written.
pub(super) fn write_chunks(
    root: &Path,
    chunks: Vec<(ChunkPosition, StoredChunk)>,
) -> io::Result<usize> {
    let mut by_region: HashMap<(i32, i32), Vec<(ChunkPosition, StoredChunk)>> = HashMap::new();
    for (position, chunk) in chunks {
        by_region
            .entry(region_of(position))
            .or_default()
            .push((position, chunk));
    }
    let mut saved = 0;
    for (region, entries) in by_region {
        let directory = region_path(root, region);
        fs::create_dir_all(&directory)?;
        for (position, chunk) in entries {
            write_chunk_file(&directory.join(chunk_file_name(position)), &chunk)?;
            saved += 1;
        }
    }
    Ok(saved)
}
