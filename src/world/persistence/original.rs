//! Beta 1.7.3's own save format, so a world can move between this game and the
//! original client or server.
//!
//! ```text
//! level.dat           gzip NBT: seed, spawn, time, weather, and the player
//! level.dat_old       the previous level.dat, as Beta keeps it
//! session.lock        8 bytes: the time the world was opened, in milliseconds
//! region/
//!   r.0.0.mcr         McRegion file: chunks 0..31 x 0..31
//!   r.-1.0.mcr        chunks -32..-1 x 0..31
//! mc-rs.json          what Beta has no field for (difficulty, game mode, flying); ignored by Beta
//! ```
//!
//! Beta stores blocks, metadata and light but not biomes, scheduled ticks or the
//! simulation state of mobs, so those are rebuilt on load: climate from the seed,
//! ticks from the world, and mob timers from defaults. Every Beta block id loads;
//! one with a shape this game does not draw yet (rails, beds, signs, redstone
//! parts, cake) shows as a stone-textured cube. Tile entities
//! other than chests, furnaces, dispensers, note blocks and spawners (signs) are
//! dropped, which is also what Beta does with ids it does not know. Falling
//! blocks and primed TNT in flight are not written, so one caught mid-fall or
//! mid-fuse by a save is lost with its block; the native format keeps them.
//! Minecarts are written as Beta's `Minecart` entity (type, a furnace cart's
//! push and fuel, a chest cart's items) and boats as its `Boat`, which has no
//! fields of its own; who rides one is not kept.
//! Neither format keeps arrows (a stuck one that could have been picked up
//! included), fireballs, thrown snowballs and eggs, or bobbers. The format
//! is described by `ChunkLoader`, `McRegionChunkLoader`, `RegionFile`, `WorldInfo`
//! and `NBTBase` in the reference source.

mod nbt;
mod region;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::log::warn;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::read::ZlibDecoder;
use flate2::write::GzEncoder;
use flate2::write::ZlibEncoder;
use serde::Deserialize;
use serde::Serialize;

use self::nbt::Compound;
use self::nbt::Tag;
use self::region::RegionFile;
use super::FORMAT_VERSION;
use super::SaveFormat;
use super::StoredPlayer;
use super::StoredStack;
use super::WorldManifest;
use crate::block::blocks::Block;
use crate::entity::SavedBody;
use crate::entity::SavedSlot;
use crate::entity::minecart::CARGO_SLOTS;
use crate::entity::minecart::Cargo;
use crate::entity::minecart::CartKind;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobRecord;
use crate::entity::mobs::MobSpawner;
use crate::entity::mobs::MobType;
use crate::item::Item;
use crate::item::ItemStack;
use crate::random::JavaRandom;
use crate::world::biome::BiomeMap;
use crate::world::chest::CHEST_SLOTS;
use crate::world::chest::Chest;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkDroppedItem;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::Heightmap;
use crate::world::chunk::NoteState;
use crate::world::difficulty::Difficulty;
use crate::world::dimension::Dimension;
use crate::world::dispenser::DISPENSER_SLOTS;
use crate::world::dispenser::Dispenser;
use crate::world::furnace::FURNACE_SLOTS;
use crate::world::furnace::Furnace;
use crate::world::furnace::SMELT_TICKS;
use crate::world::furnace::fuel_ticks;
use crate::world::generation::overworld::BiomeGenerator;
use crate::world::lighting::Skylight;
use crate::world::lighting::light_opacity;
use crate::world::lighting::unpack;
use crate::world::weather::WorldWeather;

const LEVEL_FILE: &str = "level.dat";
const LEVEL_OLD_FILE: &str = "level.dat_old";
const LEVEL_NEW_FILE: &str = "level.dat_new";
const SESSION_LOCK_FILE: &str = "session.lock";
const SIDECAR_FILE: &str = "mc-rs.json";
const REGION_DIRECTORY: &str = "region";
/// The `version` field `SaveOldDir` stamps on every `level.dat` it writes.
const BETA_LEVEL_VERSION: i32 = 19132;

const BLOCKS: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;
const NIBBLES: usize = BLOCKS / 2;
const COLUMNS: usize = CHUNK_SIZE * CHUNK_SIZE;

const HOTBAR_SLOTS: usize = 9;
const MAIN_SLOTS: usize = 27;
const ARMOR_SLOTS: usize = 4;
const INVENTORY_SLOTS: usize = HOTBAR_SLOTS + MAIN_SLOTS;
/// Beta numbers armor slots from 100, boots first.
const ARMOR_SLOT_BASE: i8 = 100;
/// A zombie pigman's `angerLevel` when it has just been angered.
const PIGMAN_ANGER: i16 = 400;
/// Where a new world's spawn point goes; the player appears over this column.
const DEFAULT_SPAWN: [i32; 3] = [8, 64, 8];

const MOB_IDS: [(&str, MobType); 13] = [
    ("Spider", MobType::Spider),
    ("Zombie", MobType::Zombie),
    ("Skeleton", MobType::Skeleton),
    ("Creeper", MobType::Creeper),
    ("Slime", MobType::Slime),
    ("Sheep", MobType::Sheep),
    ("Pig", MobType::Pig),
    ("Chicken", MobType::Chicken),
    ("Cow", MobType::Cow),
    ("Wolf", MobType::Wolf),
    ("Squid", MobType::Squid),
    ("Ghast", MobType::Ghast),
    ("PigZombie", MobType::PigZombie),
];

fn mob_id(kind: MobType) -> &'static str {
    MOB_IDS
        .iter()
        .find_map(|(id, mob)| (*mob == kind).then_some(*id))
        .unwrap_or("Pig")
}

fn mob_type(id: &str) -> Option<MobType> {
    MOB_IDS
        .iter()
        .find_map(|(name, kind)| (*name == id).then_some(*kind))
}

/// True when `root` holds a Beta world rather than a native one.
pub(super) fn is_world(root: &Path) -> bool {
    root.join(LEVEL_FILE).is_file() || root.join(LEVEL_OLD_FILE).is_file()
}

fn gzip(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes)?;
    encoder.finish()
}

fn zlib(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes)?;
    encoder.finish()
}

fn decompress(compression: u8, bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    match compression {
        region::COMPRESSION_GZIP => GzDecoder::new(bytes).read_to_end(&mut out)?,
        region::COMPRESSION_ZLIB => ZlibDecoder::new(bytes).read_to_end(&mut out)?,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unknown chunk compression",
            ));
        }
    };
    Ok(out)
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

/// A stable stand-in for the state Beta does not save, such as a mob's random
/// number generator: the same entity always gets the same one.
fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn seed_from_position(position: [f64; 3], salt: u64) -> u64 {
    mix(position[0].to_bits() ^ mix(position[1].to_bits() ^ mix(position[2].to_bits() ^ salt)))
}

// ---------------------------------------------------------------------------
// Items

fn stack_compound(stack: ItemStack) -> Compound {
    let mut compound = Compound::new();
    compound.put_short("id", stack.item().as_u16() as i16);
    compound.put_byte("Count", stack.count() as i8);
    compound.put_short("Damage", stack.data() as i16);
    compound
}

fn read_stack(compound: &Compound) -> Option<ItemStack> {
    let count = compound.byte("Count");
    if count <= 0 {
        return None;
    }
    let item = Item::from_u16(compound.short("id") as u16)?;
    let count = count as u8;
    let data = compound.short("Damage") as u16;
    ItemStack::with_data(item, count, data)
        .or_else(|_| ItemStack::with_data(item, count, 0))
        .ok()
}

fn stored_stack(stack: ItemStack) -> StoredStack {
    StoredStack {
        id: stack.item().as_u16(),
        count: stack.count(),
        data: stack.data(),
    }
}

/// A container's `Items` list: each stack with its `Slot`.
fn items_list(slots: &[Option<ItemStack>]) -> Vec<Tag> {
    slots
        .iter()
        .enumerate()
        .filter_map(|(slot, stack)| {
            let mut compound = stack_compound((*stack)?);
            compound.put_byte("Slot", slot as i8);
            Some(Tag::Compound(compound))
        })
        .collect()
}

fn read_items<const N: usize>(compound: &Compound) -> [Option<ItemStack>; N] {
    let mut slots = [None; N];
    for item in compound.compounds("Items") {
        let slot = item.byte("Slot");
        if let Some(target) = usize::try_from(slot)
            .ok()
            .and_then(|slot| slots.get_mut(slot))
        {
            *target = read_stack(item);
        }
    }
    slots
}

// ---------------------------------------------------------------------------
// Chunks

fn beta_index(x: usize, y: usize, z: usize) -> usize {
    (x << 11) | (z << 7) | y
}

fn nibble(array: &[u8], index: usize) -> u8 {
    (array[index >> 1] >> ((index & 1) * 4)) & 0x0F
}

fn set_nibble(array: &mut [u8], index: usize, value: u8) {
    array[index >> 1] |= (value & 0x0F) << ((index & 1) * 4);
}

fn world_position(chunk: ChunkPosition, index: usize) -> [i32; 3] {
    [
        chunk.x * CHUNK_SIZE as i32 + (index % CHUNK_SIZE) as i32,
        (index / (CHUNK_SIZE * CHUNK_SIZE)) as i32,
        chunk.z * CHUNK_SIZE as i32 + (index / CHUNK_SIZE % CHUNK_SIZE) as i32,
    ]
}

fn entity_base(id: &str, position: [f32; 3], motion: [f32; 3], yaw: f32) -> Compound {
    let mut entity = Compound::new();
    entity.put_string("id", id);
    entity.put_list("Pos", nbt::doubles(&position.map(f64::from)));
    entity.put_list("Motion", nbt::doubles(&motion.map(f64::from)));
    entity.put_list("Rotation", nbt::floats(&[yaw, 0.0]));
    entity.put_float("FallDistance", 0.0);
    entity.put_short("Fire", -1);
    entity.put_short("Air", 300);
    entity.put_bool("OnGround", false);
    entity
}

/// `EntityMinecart.writeEntityToNBT`: the type, then a furnace cart's push and
/// fuel or a chest cart's items. Riding and damage are not kept.
fn minecart_entity(body: &SavedBody) -> Option<Compound> {
    let SavedBody::Minecart {
        center,
        motion,
        kind,
        fuel,
        push,
        cargo,
    } = body
    else {
        return None;
    };
    let mut entity = entity_base("Minecart", *center, *motion, 0.0);
    entity.put_int("Type", kind.type_id());
    match kind {
        CartKind::Empty => {}
        CartKind::Furnace => {
            entity.put_double("PushX", f64::from(push[0]));
            entity.put_double("PushZ", f64::from(push[1]));
            entity.put_short(
                "Fuel",
                (*fuel).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            );
        }
        CartKind::Chest => {
            entity.put_list("Items", items_list(&SavedSlot::unpack(cargo).0));
        }
    }
    Some(entity)
}

/// `EntityBoat` writes nothing of its own: the base entity is the whole boat.
fn boat_entity(body: &SavedBody) -> Option<Compound> {
    let SavedBody::Boat {
        center,
        motion,
        yaw,
    } = body
    else {
        return None;
    };
    Some(entity_base("Boat", *center, *motion, *yaw))
}

fn read_boat_entity(entity: &Compound, position: [f64; 3]) -> SavedBody {
    let motion = entity.numbers::<3>("Motion").unwrap_or([0.0; 3]);
    let rotation = entity.numbers::<2>("Rotation").unwrap_or([0.0; 2]);
    SavedBody::Boat {
        center: position.map(|value| value as f32),
        motion: motion.map(|value| value as f32),
        yaw: rotation[0] as f32,
    }
}

fn read_minecart_entity(entity: &Compound, position: [f64; 3]) -> SavedBody {
    let kind = CartKind::from_type_id(entity.int("Type"));
    let motion = entity.numbers::<3>("Motion").unwrap_or([0.0; 3]);
    SavedBody::Minecart {
        center: position.map(|value| value as f32),
        motion: motion.map(|value| value as f32),
        kind,
        fuel: if kind == CartKind::Furnace {
            i32::from(entity.short("Fuel"))
        } else {
            0
        },
        push: if kind == CartKind::Furnace {
            [entity.double("PushX") as f32, entity.double("PushZ") as f32]
        } else {
            [0.0; 2]
        },
        cargo: if kind == CartKind::Chest {
            SavedSlot::pack(&Cargo(read_items::<CARGO_SLOTS>(entity)))
        } else {
            Vec::new()
        },
    }
}

fn item_entity(item: &ChunkDroppedItem) -> Compound {
    let mut entity = entity_base("Item", item.position, item.motion, 0.0);
    entity.put_short("Fire", item.fire);
    entity.put_short("Health", i16::from(item.health));
    entity.put_short("Age", item.age_ticks.min(i16::MAX as u32) as i16);
    entity.put_compound("Item", stack_compound(item.stack));
    entity
}

fn mob_entity(record: &MobRecord) -> Compound {
    let mob = &record.mob;
    let mut entity = entity_base(mob_id(mob.kind), record.feet, record.velocity, record.yaw);
    entity.put_short("Fire", mob.fire_ticks);
    entity.put_short("Health", mob.health);
    entity.put_short("HurtTime", 0);
    entity.put_short("DeathTime", 0);
    entity.put_short("AttackTime", 0);
    match mob.kind {
        MobType::Creeper if mob.charged => entity.put_bool("powered", true),
        MobType::Slime => entity.put_int("Size", i32::from(mob.variant.max(1)) - 1),
        MobType::Pig => entity.put_bool("Saddle", mob.saddled),
        MobType::Sheep => {
            entity.put_bool("Sheared", mob.sheared);
            entity.put_byte("Color", mob.variant as i8);
        }
        MobType::Wolf => {
            entity.put_bool("Angry", mob.angry);
            entity.put_bool("Sitting", mob.sitting);
            entity.put_string("Owner", mob.owner.as_deref().unwrap_or(""));
        }
        MobType::PigZombie => {
            entity.put_short("Anger", if mob.angry { PIGMAN_ANGER } else { 0 });
        }
        _ => {}
    }
    entity
}

fn read_motion(entity: &Compound) -> [f32; 3] {
    // `Entity.readFromNBT` zeroes any component that could only be corrupt.
    entity
        .numbers::<3>("Motion")
        .unwrap_or([0.0; 3])
        .map(|value| {
            if value.abs() > 10.0 {
                0.0
            } else {
                value as f32
            }
        })
}

fn read_item_entity(entity: &Compound, position: [f64; 3]) -> Option<ChunkDroppedItem> {
    let stack = read_stack(entity.compound("Item")?)?;
    let position = position.map(|value| value as f32);
    Some(ChunkDroppedItem {
        stack,
        position,
        motion: read_motion(entity),
        age_ticks: u32::try_from(entity.short("Age")).unwrap_or(0),
        pickup_delay_ticks: 0,
        hover_start: 0.0,
        rng_state: JavaRandom::new(seed_from_position(position.map(f64::from), 1)).state(),
        // `EntityItem.readEntityFromNBT` masks the short to a byte.
        health: if entity.contains("Health") {
            (entity.short("Health") & 255) as u8
        } else {
            ChunkDroppedItem::FULL_HEALTH
        },
        fire: entity.short("Fire"),
    })
}

fn read_mob_entity(entity: &Compound, kind: MobType, position: [f64; 3]) -> Option<MobRecord> {
    let health = if entity.contains("Health") {
        entity.short("Health")
    } else {
        10
    };
    if health <= 0 {
        return None;
    }
    let mut mob = Mob::new(kind, seed_from_position(position, 2));
    mob.health = health;
    mob.fire_ticks = entity.short("Fire").max(0);
    match kind {
        MobType::Creeper => mob.charged = entity.boolean("powered"),
        MobType::Slime => {
            mob.variant = (entity.int("Size").clamp(0, 126) + 1) as u8;
        }
        MobType::Pig => mob.saddled = entity.boolean("Saddle"),
        MobType::Sheep => {
            mob.sheared = entity.boolean("Sheared");
            mob.variant = (entity.byte("Color") & 15) as u8;
        }
        MobType::Wolf => {
            mob.angry = entity.boolean("Angry");
            mob.sitting = entity.boolean("Sitting");
            let owner = entity.string("Owner");
            mob.tamed = !owner.is_empty();
            mob.owner = (!owner.is_empty()).then(|| owner.to_owned());
        }
        MobType::PigZombie => mob.angry = entity.short("Anger") > 0,
        _ => {}
    }
    let yaw = entity
        .numbers::<2>("Rotation")
        .map_or(0.0, |angles| angles[0] as f32);
    Some(MobRecord {
        mob,
        feet: position.map(|value| value as f32),
        velocity: read_motion(entity),
        yaw,
    })
}

/// `Chunk.getChunkData` for a whole chunk: what `Packet51MapChunk` carries
/// before it is deflated. Blocks, then the metadata, block light and sky light
/// nibbles, each in Beta's `x << 11 | z << 7 | y` order.
///
/// `light` is the chunk's cells from its last mesh job; without them the
/// chunk is lit alone.
pub fn beta_chunk_bytes(chunk: &Chunk, light: Option<&[u8]>, has_sky: bool) -> Vec<u8> {
    let raw = chunk.raw_blocks();
    let metadata = chunk.raw_metadata();
    let lit_alone;
    let light = match light.filter(|cells| cells.len() == BLOCKS) {
        Some(cells) => cells,
        None => {
            lit_alone = Skylight::from_chunk_enclosed(chunk).chunk_cells();
            &lit_alone
        }
    };
    let mut bytes = vec![0u8; BLOCKS + 3 * NIBBLES];
    let (blocks, rest) = bytes.split_at_mut(BLOCKS);
    let (data, rest) = rest.split_at_mut(NIBBLES);
    let (block_light, sky) = rest.split_at_mut(NIBBLES);
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                let ours = Chunk::index(x, y, z);
                let beta = beta_index(x, y, z);
                blocks[beta] = raw[ours];
                if let Some(metadata) = metadata {
                    set_nibble(data, beta, nibble(metadata, ours));
                }
                let (sky_level, block_level) = unpack(light[ours]);
                if has_sky {
                    set_nibble(sky, beta, sky_level);
                }
                set_nibble(block_light, beta, block_level);
            }
        }
    }
    bytes
}

/// A chunk copied out of the live world, waiting to be encoded and written.
///
/// Taking the copy is cheap (blocks are shared), so it happens on the main
/// thread. The NBT, the light and the compression happen on the writer.
pub(super) struct ChunkSnapshot {
    position: ChunkPosition,
    chunk: Chunk,
    items: Vec<ChunkDroppedItem>,
    populated: bool,
    /// The chunk's light from its last mesh job, in [`Chunk::index`] order,
    /// when it had one.
    light: Option<Arc<[u8]>>,
}

impl ChunkSnapshot {
    pub(super) fn from_generated(
        position: ChunkPosition,
        generated: &GeneratedChunk,
        items: &[ChunkDroppedItem],
        light: Option<Arc<[u8]>>,
    ) -> Self {
        Self {
            position,
            chunk: generated.chunk.clone(),
            items: items.to_vec(),
            populated: generated.populated,
            light: light.filter(|cells| cells.len() == BLOCKS),
        }
    }

    /// The chunk as a zlib-compressed NBT document, like Beta writes into a
    /// region file.
    fn to_compressed(&self, time: i64, dimension: Dimension) -> io::Result<Vec<u8>> {
        let chunk = &self.chunk;
        let raw = chunk.raw_blocks();
        let metadata = chunk.raw_metadata();
        // Beta does not relight a chunk it loads, so save the light the chunk
        // had among its neighbors. A chunk that was never meshed is lit alone,
        // with dark borders rather than sky leaking in through them.
        let light = match &self.light {
            Some(cells) => Arc::clone(cells),
            None => Skylight::from_chunk_enclosed(chunk).chunk_cells(),
        };

        let mut blocks = vec![0u8; BLOCKS];
        let mut data = vec![0u8; NIBBLES];
        let mut sky = vec![0u8; NIBBLES];
        let mut block_light = vec![0u8; NIBBLES];
        let mut height_map = vec![0u8; COLUMNS];
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let mut height = None;
                for y in (0..CHUNK_HEIGHT).rev() {
                    let ours = Chunk::index(x, y, z);
                    let beta = beta_index(x, y, z);
                    blocks[beta] = raw[ours];
                    if let Some(metadata) = metadata {
                        set_nibble(&mut data, beta, nibble(metadata, ours));
                    }
                    let (sky_level, block_level) = unpack(light[ours]);
                    // `hasNoSky`: the Nether's sky light array stays zero.
                    if dimension.has_sky() {
                        set_nibble(&mut sky, beta, sky_level);
                    }
                    set_nibble(&mut block_light, beta, block_level);
                    if height.is_none()
                        && Block::from_u8(raw[ours]).is_none_or(|b| light_opacity(b) > 0)
                    {
                        height = Some(y + 1);
                    }
                }
                height_map[z << 4 | x] = height.unwrap_or(0) as u8;
            }
        }

        let mut level = Compound::new();
        level.put_int("xPos", self.position.x);
        level.put_int("zPos", self.position.z);
        level.put_long("LastUpdate", time);
        level.put_bytes("Blocks", blocks);
        level.put_bytes("Data", data);
        level.put_bytes("SkyLight", sky);
        level.put_bytes("BlockLight", block_light);
        level.put_bytes("HeightMap", height_map);
        level.put_bool("TerrainPopulated", self.populated);

        let mut entities: Vec<Tag> = self
            .items
            .iter()
            .map(|item| Tag::Compound(item_entity(item)))
            .collect();
        entities.extend(
            chunk
                .mob_records()
                .iter()
                .map(|record| Tag::Compound(mob_entity(record))),
        );
        entities.extend(
            chunk
                .saved_bodies()
                .iter()
                .filter_map(|body| minecart_entity(body).or_else(|| boat_entity(body)))
                .map(Tag::Compound),
        );
        level.put_list("Entities", entities);

        let tile_entity = |id: &str, index: usize| {
            let [x, y, z] = world_position(self.position, index);
            let mut tile = Compound::new();
            tile.put_string("id", id);
            tile.put_int("x", x);
            tile.put_int("y", y);
            tile.put_int("z", z);
            tile
        };
        let mut tiles = Vec::new();
        for (index, chest) in chunk.chests() {
            let mut tile = tile_entity("Chest", index);
            tile.put_list("Items", items_list(&chest.slots));
            tiles.push(Tag::Compound(tile));
        }
        for (index, dispenser) in chunk.dispensers() {
            let mut tile = tile_entity("Trap", index);
            tile.put_list("Items", items_list(&dispenser.slots));
            tiles.push(Tag::Compound(tile));
        }
        for (index, note) in chunk.notes() {
            let mut tile = tile_entity("Music", index);
            tile.put_byte("note", note.pitch as i8);
            tiles.push(Tag::Compound(tile));
        }
        for (index, furnace) in chunk.furnaces() {
            let mut tile = tile_entity("Furnace", index);
            tile.put_short("BurnTime", furnace.burn_ticks as i16);
            tile.put_short("CookTime", furnace.cook_ticks as i16);
            tile.put_list("Items", items_list(&furnace.slots));
            tiles.push(Tag::Compound(tile));
        }
        for (index, spawner) in chunk.spawners() {
            let mut tile = tile_entity("MobSpawner", index);
            tile.put_string("EntityId", mob_id(spawner.kind));
            tile.put_short("Delay", spawner.delay.min(i16::MAX as u16) as i16);
            tiles.push(Tag::Compound(tile));
        }
        level.put_list("TileEntities", tiles);

        let mut root = Compound::new();
        root.put_compound("Level", level);
        zlib(&nbt::write_root(&root))
    }
}

fn decode_chunk(
    level: &Compound,
    position: ChunkPosition,
    biomes: Option<&BiomeGenerator>,
) -> Option<GeneratedChunk> {
    // Beta logs a mismatched chunk and carries on; one that claims to be
    // somewhere else is not trustworthy, so it is regenerated instead.
    if level.contains("xPos")
        && (level.int("xPos") != position.x || level.int("zPos") != position.z)
    {
        return None;
    }
    let beta_blocks = level.bytes("Blocks");
    if beta_blocks.len() != BLOCKS {
        return None;
    }
    let beta_data = level.bytes("Data");
    let has_data = beta_data.len() == NIBBLES;

    // `removeUnknownBlocks`, widened to the blocks this game simulates.
    let mut known = [false; 256];
    for (id, known) in known.iter_mut().enumerate() {
        *known = Block::from_u8(id as u8).is_some();
    }

    let mut blocks = vec![0u8; BLOCKS];
    let mut metadata = vec![0u8; NIBBLES];
    let mut has_metadata = false;
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                let beta = beta_index(x, y, z);
                let id = beta_blocks[beta];
                if !known[usize::from(id)] {
                    continue;
                }
                let ours = Chunk::index(x, y, z);
                blocks[ours] = id;
                if has_data {
                    let value = nibble(beta_data, beta);
                    if value != 0 {
                        set_nibble(&mut metadata, ours, value);
                        has_metadata = true;
                    }
                }
            }
        }
    }
    let mut chunk = Chunk::from_raw(blocks);
    if has_metadata {
        chunk.set_raw_metadata(metadata);
    }

    for tile in level.compounds("TileEntities") {
        let local = [
            tile.int("x") - position.x * CHUNK_SIZE as i32,
            tile.int("y"),
            tile.int("z") - position.z * CHUNK_SIZE as i32,
        ];
        let in_range = local[0] >= 0
            && local[0] < CHUNK_SIZE as i32
            && local[1] >= 0
            && local[1] < CHUNK_HEIGHT as i32
            && local[2] >= 0
            && local[2] < CHUNK_SIZE as i32;
        if !in_range {
            continue;
        }
        let (x, y, z) = (local[0] as usize, local[1] as usize, local[2] as usize);
        let index = Chunk::index(x, y, z);
        let block = chunk.get(x, y, z);
        match tile.string("id") {
            "Chest" if block.is_some_and(Block::is_chest) => {
                let slots = read_items::<CHEST_SLOTS>(tile);
                chunk.insert_chest(index, Chest { slots });
            }
            "Trap" if block == Some(Block::Dispenser) => {
                let slots = read_items::<DISPENSER_SLOTS>(tile);
                chunk.insert_dispenser(index, Dispenser { slots });
            }
            "Music" if block == Some(Block::NoteBlock) => {
                chunk.insert_note(
                    index,
                    NoteState {
                        pitch: tile.byte("note").clamp(0, 24) as u8,
                        previous_powered: false,
                    },
                );
            }
            "Furnace" if block.is_some_and(Block::is_furnace) => {
                let slots = read_items::<FURNACE_SLOTS>(tile);
                let burn_ticks = tile.short("BurnTime").max(0) as u16;
                let fuel = slots[1].and_then(fuel_ticks);
                chunk.insert_furnace(
                    index,
                    Furnace {
                        slots,
                        burn_ticks,
                        fuel_ticks: if burn_ticks > 0 {
                            fuel.unwrap_or(burn_ticks).max(burn_ticks)
                        } else {
                            0
                        },
                        cook_ticks: (tile.short("CookTime").max(0) as u16).min(SMELT_TICKS - 1),
                    },
                );
            }
            "MobSpawner" if block == Some(Block::MobSpawner) => {
                let default = MobSpawner::default();
                chunk.insert_spawner(
                    index,
                    MobSpawner {
                        kind: mob_type(tile.string("EntityId")).unwrap_or(default.kind),
                        delay: tile.short("Delay").max(0) as u16,
                        rng_state: default.rng_state,
                    },
                );
            }
            _ => {}
        }
    }

    let mut items = Vec::new();
    let mut mobs = Vec::new();
    let mut carts = Vec::new();
    for entity in level.compounds("Entities") {
        let Some(position) = entity.numbers::<3>("Pos") else {
            continue;
        };
        match entity.string("id") {
            "Item" => items.extend(read_item_entity(entity, position)),
            "Minecart" => carts.push(read_minecart_entity(entity, position)),
            "Boat" => carts.push(read_boat_entity(entity, position)),
            id => {
                if let Some(kind) = mob_type(id) {
                    mobs.extend(read_mob_entity(entity, kind, position));
                }
            }
        }
    }
    chunk.set_mob_records(mobs);
    chunk.set_saved_bodies(carts);

    Some(GeneratedChunk {
        heightmap: Heightmap::from_chunk(&chunk),
        chunk,
        // The Nether is one biome; the Overworld's climate comes from the seed.
        biomes: biomes.map_or_else(BiomeMap::hell, |biomes| biomes.generate(position)),
        items,
        populated: level.boolean("TerrainPopulated"),
    })
}

// ---------------------------------------------------------------------------
// The world's files

/// Data this game needs that Beta has no field for. Beta ignores the file.
#[derive(Default, Serialize, Deserialize)]
struct Sidecar {
    #[serde(default)]
    difficulty: Option<Difficulty>,
    #[serde(default)]
    created_unix_millis: u64,
    #[serde(default)]
    flying: bool,
    #[serde(default)]
    fly_speed: f32,
    #[serde(default)]
    game_mode: crate::player::GameMode,
    #[serde(default)]
    selected: usize,
}

fn read_sidecar(root: &Path) -> Sidecar {
    fs::read(root.join(SIDECAR_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_sidecar(root: &Path, sidecar: &Sidecar) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(sidecar).map_err(io::Error::other)?;
    let path = root.join(SIDECAR_FILE);
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

fn read_level_file(path: &Path) -> io::Result<Compound> {
    let bytes = fs::read(path)?;
    let mut decoded = Vec::new();
    GzDecoder::new(bytes.as_slice()).read_to_end(&mut decoded)?;
    nbt::read_root(&decoded)
}

/// The root compound of `level.dat`, falling back to `level.dat_old` like Beta.
fn read_level_root(root: &Path) -> io::Result<Compound> {
    read_level_file(&root.join(LEVEL_FILE))
        .or_else(|first| read_level_file(&root.join(LEVEL_OLD_FILE)).map_err(|_| first))
}

/// What `level.dat` and the sidecar say about a world.
pub(super) struct Level {
    pub manifest: WorldManifest,
    pub player: Option<StoredPlayer>,
}

pub(super) fn read_level(root: &Path) -> io::Result<Level> {
    let level = read_level_root(root)?;
    let data = level.compound("Data").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "level.dat has no Data compound")
    })?;
    let sidecar = read_sidecar(root);

    let last_played = u64::try_from(data.long("LastPlayed")).unwrap_or(0);
    let created = if sidecar.created_unix_millis > 0 {
        sidecar.created_unix_millis
    } else {
        fs::metadata(root)
            .and_then(|metadata| metadata.created().or_else(|_| metadata.modified()))
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(last_played, |duration| duration.as_millis() as u64)
    };
    let name = match data.string("LevelName") {
        "" => root.file_name().map_or_else(
            || "World".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        ),
        name => name.to_owned(),
    };
    let raining = data.boolean("raining");
    let thundering = data.boolean("thundering");
    let weather = WorldWeather {
        raining,
        thundering,
        rain_time: u32::try_from(data.int("rainTime")).unwrap_or(0),
        thunder_time: u32::try_from(data.int("thunderTime")).unwrap_or(0),
        rain_strength: if raining { 1.0 } else { 0.0 },
        thunder_strength: if thundering { 1.0 } else { 0.0 },
        ..WorldWeather::default()
    };
    let player = data
        .compound("Player")
        .and_then(|player| player_from_nbt(player, &sidecar));
    Ok(Level {
        manifest: WorldManifest {
            name,
            seed: data.long("RandomSeed") as u64,
            created_unix_millis: created,
            last_played_unix_millis: last_played.max(created),
            format_version: FORMAT_VERSION,
            world_time: u64::try_from(data.long("Time")).unwrap_or(0),
            weather,
            difficulty: sidecar.difficulty,
            format: SaveFormat::Original,
        },
        player,
    })
}

/// Write `level.dat` and the sidecar. Fields this game does not model, such as
/// the spawn point and anything else already in the file, are kept as they are.
/// With no `player` the stored one is left untouched.
pub(super) fn write_level(
    root: &Path,
    manifest: &WorldManifest,
    player: Option<&StoredPlayer>,
) -> io::Result<()> {
    let mut level = read_level_root(root).unwrap_or_default();
    let mut data = level.compound("Data").cloned().unwrap_or_default();
    if !data.contains("SpawnX") {
        data.put_int("SpawnX", DEFAULT_SPAWN[0]);
        data.put_int("SpawnY", DEFAULT_SPAWN[1]);
        data.put_int("SpawnZ", DEFAULT_SPAWN[2]);
    }
    data.put_long("RandomSeed", manifest.seed as i64);
    data.put_long("Time", manifest.world_time as i64);
    data.put_long("LastPlayed", manifest.last_played_unix_millis as i64);
    data.put_long("SizeOnDisk", 0);
    data.put_string("LevelName", &manifest.name);
    data.put_int("version", BETA_LEVEL_VERSION);
    let weather = &manifest.weather;
    data.put_bool("raining", weather.raining);
    data.put_int("rainTime", weather.rain_time.min(i32::MAX as u32) as i32);
    data.put_bool("thundering", weather.thundering);
    data.put_int(
        "thunderTime",
        weather.thunder_time.min(i32::MAX as u32) as i32,
    );

    let mut sidecar = read_sidecar(root);
    sidecar.difficulty = manifest.difficulty;
    sidecar.created_unix_millis = manifest.created_unix_millis;
    if let Some(player) = player {
        let base = data.compound("Player").cloned().unwrap_or_default();
        data.put_compound("Player", player_to_nbt(player, base));
        sidecar.flying = player.flying;
        sidecar.fly_speed = player.fly_speed;
        sidecar.game_mode = player.game_mode;
        sidecar.selected = player.selected;
    }
    level.put_compound("Data", data);

    // Beta writes a new file, retires the old one, then renames, so a crash
    // leaves either the old or the new `level.dat` intact.
    let compressed = gzip(&nbt::write_root(&level))?;
    let (new, current, old) = (
        root.join(LEVEL_NEW_FILE),
        root.join(LEVEL_FILE),
        root.join(LEVEL_OLD_FILE),
    );
    fs::write(&new, compressed)?;
    match fs::remove_file(&old) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    match fs::rename(&current, &old) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    fs::rename(&new, &current)?;
    write_sidecar(root, &sidecar)
}

/// `PlayerNBTManager`: where a Beta server keeps each player by name.
fn named_player_path(root: &Path, name: &str) -> PathBuf {
    root.join("players").join(format!("{name}.dat"))
}

/// A named player's record, as a Beta server stores it. The sidecar's fields
/// (flight, game mode, the selected slot) belong to the world's own player,
/// so a named one comes back with their defaults.
pub(super) fn read_named_player(root: &Path, name: &str) -> Option<StoredPlayer> {
    let player = read_level_file(&named_player_path(root, name)).ok()?;
    player_from_nbt(&player, &Sidecar::default())
}

pub(super) fn write_named_player(root: &Path, name: &str, player: &StoredPlayer) -> io::Result<()> {
    let path = named_player_path(root, name);
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    // Keep whatever tags Beta wrote that this game does not model.
    let base = read_level_file(&path).unwrap_or_default();
    let compressed = gzip(&nbt::write_root(&player_to_nbt(player, base)))?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, compressed)?;
    fs::rename(&temporary, path)
}

/// Claim the world like Beta does: its lock holds the time it was opened. This
/// game never checks the lock, since a second session saving over the first
/// is the player's doing, so a failure only warns.
fn write_session_lock(root: &Path) {
    if let Err(error) = fs::write(root.join(SESSION_LOCK_FILE), unix_millis().to_be_bytes()) {
        warn!("Could not write session.lock: {error}");
    }
}

// ---------------------------------------------------------------------------
// The player

/// `StoredPlayer` and Beta disagree about which way is forward. Beta's yaw is 0
/// facing south (+z) and grows toward the west, in degrees; its pitch is
/// positive looking down. Bevy's yaw is 0 facing north (-z) and grows toward
/// the west too, and its pitch is positive looking up.
fn yaw_from_beta(degrees: f64) -> f32 {
    (std::f64::consts::PI - degrees.to_radians()) as f32
}

fn yaw_to_beta(radians: f32) -> f32 {
    (std::f64::consts::PI - f64::from(radians))
        .to_degrees()
        .rem_euclid(360.0) as f32
}

fn player_from_nbt(player: &Compound, sidecar: &Sidecar) -> Option<StoredPlayer> {
    let [x, y, z] = player.numbers::<3>("Pos")?;
    let [yaw, pitch] = player.numbers::<2>("Rotation").unwrap_or([0.0; 2]);
    let health = if player.contains("Health") {
        player.short("Health")
    } else {
        i16::from(crate::player::MAX_PLAYER_HEALTH)
    };
    let health = if health > 0 {
        health.min(i16::from(crate::player::MAX_PLAYER_HEALTH)) as u8
    } else {
        crate::player::MAX_PLAYER_HEALTH
    };

    let mut hotbar = vec![None; HOTBAR_SLOTS];
    let mut main = vec![None; MAIN_SLOTS];
    let mut armor = vec![None; ARMOR_SLOTS];
    for item in player.compounds("Inventory") {
        let Some(stack) = read_stack(item).map(stored_stack) else {
            continue;
        };
        let slot = i16::from(item.byte("Slot"));
        let target = match slot {
            0..=8 => hotbar.get_mut(slot as usize),
            9..=35 => main.get_mut(slot as usize - HOTBAR_SLOTS),
            100..=103 => {
                armor.get_mut(ARMOR_SLOTS - 1 - (slot - i16::from(ARMOR_SLOT_BASE)) as usize)
            }
            _ => None,
        };
        if let Some(target) = target {
            *target = Some(stack);
        }
    }
    Some(StoredPlayer {
        format_version: FORMAT_VERSION,
        x: x as f32,
        y: y as f32,
        z: z as f32,
        yaw: yaw_from_beta(yaw),
        pitch: (-pitch.to_radians()) as f32,
        health,
        air: if player.contains("Air") {
            player.short("Air")
        } else {
            crate::entity::creature::MAX_AIR
        },
        fire: if player.contains("Fire") {
            player.short("Fire")
        } else {
            -20
        },
        fall_distance: player.float("FallDistance"),
        hotbar,
        selected: sidecar.selected.min(HOTBAR_SLOTS - 1),
        main,
        crafting: Vec::new(),
        armor,
        carried: None,
        flying: sidecar.flying,
        game_mode: sidecar.game_mode,
        fly_speed: if sidecar.fly_speed > 0.0 {
            sidecar.fly_speed
        } else {
            1.0
        },
        dimension: Dimension::from_id(player.int("Dimension")),
        spawn: player
            .contains("SpawnX")
            .then(|| ["SpawnX", "SpawnY", "SpawnZ"].map(|name| player.int(name))),
    })
}

/// The player's `Inventory` list. Beta saves no crafting grid or held stack, so
/// those go into free slots rather than vanishing.
fn inventory_list(player: &StoredPlayer) -> Vec<Tag> {
    let mut slots: [Option<StoredStack>; INVENTORY_SLOTS] = [None; INVENTORY_SLOTS];
    for (index, stack) in player.hotbar.iter().take(HOTBAR_SLOTS).enumerate() {
        slots[index] = *stack;
    }
    for (index, stack) in player.main.iter().take(MAIN_SLOTS).enumerate() {
        slots[HOTBAR_SLOTS + index] = *stack;
    }
    let loose = player
        .crafting
        .iter()
        .copied()
        .flatten()
        .chain(player.carried);
    for stack in loose {
        if let Some(free) = slots.iter_mut().find(|slot| slot.is_none()) {
            *free = Some(stack);
        }
    }

    let tag = |stack: StoredStack, slot: i8| {
        let mut compound = Compound::new();
        compound.put_byte("Slot", slot);
        compound.put_short("id", stack.id as i16);
        compound.put_byte("Count", stack.count as i8);
        compound.put_short("Damage", stack.data as i16);
        Tag::Compound(compound)
    };
    let mut list: Vec<Tag> = slots
        .iter()
        .enumerate()
        .filter_map(|(slot, stack)| Some(tag((*stack)?, slot as i8)))
        .collect();
    for (index, stack) in player.armor.iter().take(ARMOR_SLOTS).enumerate() {
        if let Some(stack) = stack {
            list.push(tag(
                *stack,
                ARMOR_SLOT_BASE + (ARMOR_SLOTS - 1 - index) as i8,
            ));
        }
    }
    list
}

/// `player` laid over `base`, so fields this game does not track (the score)
/// survive a save.
fn player_to_nbt(player: &StoredPlayer, mut base: Compound) -> Compound {
    base.put_list(
        "Pos",
        nbt::doubles(&[player.x, player.y, player.z].map(f64::from)),
    );
    base.put_list("Motion", nbt::doubles(&[0.0; 3]));
    base.put_list(
        "Rotation",
        nbt::floats(&[yaw_to_beta(player.yaw), -player.pitch.to_degrees()]),
    );
    base.put_float("FallDistance", player.fall_distance);
    base.put_short("Fire", player.fire);
    base.put_short("Air", player.air);
    base.put_bool("OnGround", true);
    base.put_short("Health", i16::from(player.health));
    base.put_short("HurtTime", 0);
    base.put_short("DeathTime", 0);
    base.put_short("AttackTime", 0);
    base.put_int("Dimension", player.dimension.id());
    base.put_list("Inventory", inventory_list(player));
    // A sleeper is saved awake, as Beta wakes one when it loads.
    base.put_bool("Sleeping", false);
    base.put_short("SleepTimer", 0);
    for (name, value) in ["SpawnX", "SpawnY", "SpawnZ"]
        .into_iter()
        .zip(player.spawn.map_or([None; 3], |spawn| spawn.map(Some)))
    {
        match value {
            Some(value) => base.put_int(name, value),
            None => base.remove(name),
        }
    }
    base
}

// ---------------------------------------------------------------------------
// The store

/// A Beta world folder. Region files stay open between loads and saves, as
/// Beta's `RegionFileCache` keeps them.
///
/// One lock guards the cache, but compressing and decompressing happen outside
/// it, so a load on a generation thread only waits for the disk read.
pub(super) struct OriginalStore {
    root: PathBuf,
    regions: Mutex<HashMap<(Dimension, i32, i32), RegionFile>>,
    biomes: BiomeGenerator,
}

impl OriginalStore {
    /// Make a new world folder's files.
    pub(super) fn create(root: &Path, manifest: &WorldManifest) -> io::Result<Self> {
        fs::create_dir_all(root.join(REGION_DIRECTORY))?;
        write_level(root, manifest, None)?;
        Ok(Self::new(root, manifest.seed))
    }

    pub(super) fn open(root: &Path, seed: u64) -> Self {
        Self::new(root, seed)
    }

    fn new(root: &Path, seed: u64) -> Self {
        write_session_lock(root);
        Self {
            root: root.to_owned(),
            regions: Mutex::new(HashMap::new()),
            biomes: BiomeGenerator::new(seed),
        }
    }

    /// `SaveOldDir.getChunkLoader`: the Nether's regions live in `DIM-1`.
    fn region_directory(&self, dimension: Dimension) -> PathBuf {
        match dimension.folder() {
            Some(folder) => self.root.join(folder).join(REGION_DIRECTORY),
            None => self.root.join(REGION_DIRECTORY),
        }
    }

    fn region_path(&self, dimension: Dimension, region: (i32, i32)) -> PathBuf {
        self.region_directory(dimension)
            .join(format!("r.{}.{}.mcr", region.0, region.1))
    }

    /// Run `action` on the region file holding `position`, opening it first. A
    /// region that does not exist is made only when `create` is set.
    fn with_region<T>(
        &self,
        dimension: Dimension,
        position: ChunkPosition,
        create: bool,
        action: impl FnOnce(&mut RegionFile) -> io::Result<T>,
    ) -> io::Result<Option<T>> {
        let key = (dimension, position.x >> 5, position.z >> 5);
        let mut regions = self.regions.lock().unwrap();
        if !regions.contains_key(&key) {
            if create {
                fs::create_dir_all(self.region_directory(dimension))?;
            }
            let path = self.region_path(dimension, (key.1, key.2));
            let Some(file) = RegionFile::open(&path, create)? else {
                return Ok(None);
            };
            regions.insert(key, file);
        }
        let region = regions.get_mut(&key).expect("region was just inserted");
        action(region).map(Some)
    }

    pub(super) fn load_chunk(
        &self,
        dimension: Dimension,
        position: ChunkPosition,
    ) -> Option<GeneratedChunk> {
        let stored = self
            .with_region(dimension, position, false, |region| {
                region.read(position.x, position.z)
            })
            .map_err(|error| warn!("Ignoring unreadable region at {position:?}: {error}"))
            .ok()??;
        let (compression, payload) = stored?;
        let decoded = decompress(compression, &payload)
            .and_then(|bytes| nbt::read_root(&bytes))
            .map_err(|error| warn!("Ignoring unreadable chunk {position:?}: {error}"))
            .ok()?;
        let biomes = (dimension == Dimension::Overworld).then_some(&self.biomes);
        decode_chunk(decoded.compound("Level")?, position, biomes)
    }

    /// Encode and write chunks. Returns how many were written; a chunk too big
    /// for a region file is skipped with a warning, as Beta drops it.
    pub(super) fn write_chunks<'a>(
        &self,
        dimension: Dimension,
        chunks: impl IntoIterator<Item = (ChunkPosition, &'a ChunkSnapshot)>,
        world_time: u64,
    ) -> io::Result<usize> {
        let mut saved = 0;
        for (position, snapshot) in chunks {
            let compressed = snapshot.to_compressed(world_time as i64, dimension)?;
            let written = self.with_region(dimension, position, true, |region| {
                region.write(position.x, position.z, &compressed)
            });
            match written {
                Ok(_) => saved += 1,
                Err(error) if error.kind() == io::ErrorKind::Other => {
                    warn!("Skipping chunk {position:?}: {error}");
                }
                Err(error) => return Err(error),
            }
        }
        Ok(saved)
    }
}
