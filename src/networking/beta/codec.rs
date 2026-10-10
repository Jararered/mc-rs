//! Reading the packets a Beta 1.7.3 client sends and writing the ones it
//! reads. Everything is big-endian, as `DataInputStream` has it, and a string
//! is a short count of UTF-16 code units followed by the units.

use bevy::math::Vec3;

/// Why a packet could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The rest of it has not arrived yet.
    Incomplete,
    /// The stream is not one this server understands.
    Invalid(String),
}

/// `Packet15Place` and `Packet102WindowClick`'s item: id, count, damage.
pub type WireStack = (i16, i8, i16);

/// A packet from the client. Fields the server has no use for are read and
/// dropped.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientPacket {
    KeepAlive,
    Login {
        protocol: i32,
        username: String,
    },
    Handshake {
        username: String,
    },
    Chat(String),
    UseEntity {
        target: i32,
        attack: bool,
    },
    Respawn,
    /// `Packet10Flying` and its subclasses. A part the packet does not carry
    /// is `None`.
    Move {
        /// `x`, `y` (the feet), `stance` (the eyes), `z`.
        position: Option<[f64; 4]>,
        /// Yaw and pitch, in Beta's degrees.
        look: Option<[f32; 2]>,
        on_ground: bool,
    },
    Dig {
        status: u8,
        x: i32,
        y: i32,
        z: i32,
        face: u8,
    },
    Place {
        x: i32,
        y: i32,
        z: i32,
        direction: u8,
        held: Option<WireStack>,
    },
    HeldSlot(i16),
    Animation,
    EntityAction {
        state: i8,
    },
    VehicleInput,
    CloseWindow(i8),
    /// `Packet102WindowClick`.
    WindowClick {
        window: i8,
        /// -999 is a click outside the window.
        slot: i16,
        /// 0 is the left button, 1 the right.
        button: i8,
        /// The client's number for this click, echoed in the answer.
        action: i16,
        shift: bool,
        /// What the client believes was in the slot it clicked.
        clicked: Option<WireStack>,
    },
    /// `Packet106Transaction`: the client has seen that a click was refused.
    Transaction {
        window: i8,
        action: i16,
    },
    /// `Packet130UpdateSign`: what the client wrote on a sign.
    UpdateSign {
        x: i32,
        y: i32,
        z: i32,
        lines: [String; 4],
    },
    Disconnect(String),
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], ReadError> {
        let end = self.at + N;
        let slice = self.bytes.get(self.at..end).ok_or(ReadError::Incomplete)?;
        self.at = end;
        Ok(slice.try_into().expect("the slice is N long"))
    }

    fn u8(&mut self) -> Result<u8, ReadError> {
        Ok(self.take::<1>()?[0])
    }

    fn i8(&mut self) -> Result<i8, ReadError> {
        Ok(i8::from_be_bytes(self.take()?))
    }

    fn i16(&mut self) -> Result<i16, ReadError> {
        Ok(i16::from_be_bytes(self.take()?))
    }

    fn i32(&mut self) -> Result<i32, ReadError> {
        Ok(i32::from_be_bytes(self.take()?))
    }

    fn i64(&mut self) -> Result<i64, ReadError> {
        Ok(i64::from_be_bytes(self.take()?))
    }

    fn f32(&mut self) -> Result<f32, ReadError> {
        Ok(f32::from_be_bytes(self.take()?))
    }

    fn f64(&mut self) -> Result<f64, ReadError> {
        Ok(f64::from_be_bytes(self.take()?))
    }

    /// `Packet.readString(stream, max)`.
    fn string(&mut self, max: i16) -> Result<String, ReadError> {
        let length = self.i16()?;
        if !(0..=max).contains(&length) {
            return Err(ReadError::Invalid(format!(
                "a string of {length} where at most {max} fit"
            )));
        }
        let mut units = Vec::with_capacity(length as usize);
        for _ in 0..length {
            units.push(u16::from_be_bytes(self.take()?));
        }
        Ok(String::from_utf16_lossy(&units))
    }

    fn stack(&mut self) -> Result<Option<WireStack>, ReadError> {
        let id = self.i16()?;
        if id < 0 {
            return Ok(None);
        }
        Ok(Some((id, self.i8()?, self.i16()?)))
    }
}

/// Read one packet from the front of `bytes`: the packet and how many bytes
/// it took.
pub fn read_packet(bytes: &[u8]) -> Result<(ClientPacket, usize), ReadError> {
    let mut reader = Reader { bytes, at: 0 };
    let r = &mut reader;
    let id = r.u8()?;
    let packet = match id {
        0 => ClientPacket::KeepAlive,
        1 => {
            let protocol = r.i32()?;
            let username = r.string(16)?;
            r.i64()?;
            r.i8()?;
            ClientPacket::Login { protocol, username }
        }
        2 => ClientPacket::Handshake {
            username: r.string(32)?,
        },
        3 => ClientPacket::Chat(r.string(119)?),
        7 => {
            r.i32()?;
            let target = r.i32()?;
            ClientPacket::UseEntity {
                target,
                attack: r.i8()? != 0,
            }
        }
        9 => {
            r.i8()?;
            ClientPacket::Respawn
        }
        10..=13 => {
            let position = if id == 11 || id == 13 {
                Some([r.f64()?, r.f64()?, r.f64()?, r.f64()?])
            } else {
                None
            };
            let look = if id == 12 || id == 13 {
                Some([r.f32()?, r.f32()?])
            } else {
                None
            };
            ClientPacket::Move {
                position,
                look,
                on_ground: r.u8()? != 0,
            }
        }
        14 => ClientPacket::Dig {
            status: r.u8()?,
            x: r.i32()?,
            y: i32::from(r.u8()?),
            z: r.i32()?,
            face: r.u8()?,
        },
        15 => ClientPacket::Place {
            x: r.i32()?,
            y: i32::from(r.u8()?),
            z: r.i32()?,
            direction: r.u8()?,
            held: r.stack()?,
        },
        16 => ClientPacket::HeldSlot(r.i16()?),
        18 => {
            r.i32()?;
            r.i8()?;
            ClientPacket::Animation
        }
        19 => {
            r.i32()?;
            ClientPacket::EntityAction { state: r.i8()? }
        }
        27 => {
            r.take::<18>()?;
            ClientPacket::VehicleInput
        }
        101 => ClientPacket::CloseWindow(r.i8()?),
        102 => ClientPacket::WindowClick {
            window: r.i8()?,
            slot: r.i16()?,
            button: r.i8()?,
            action: r.i16()?,
            shift: r.u8()? != 0,
            clicked: r.stack()?,
        },
        106 => {
            let window = r.i8()?;
            let action = r.i16()?;
            r.u8()?;
            ClientPacket::Transaction { window, action }
        }
        130 => {
            let x = r.i32()?;
            let y = i32::from(r.i16()?);
            let z = r.i32()?;
            ClientPacket::UpdateSign {
                x,
                y,
                z,
                lines: [r.string(15)?, r.string(15)?, r.string(15)?, r.string(15)?],
            }
        }
        255 => ClientPacket::Disconnect(r.string(100)?),
        other => return Err(ReadError::Invalid(format!("packet id {other}"))),
    };
    Ok((packet, reader.at))
}

/// Packets for the client, appended to a byte buffer.
#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn i16(&mut self, value: i16) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    /// `Packet.writeString`.
    fn string(&mut self, value: &str) {
        let units: Vec<u16> = value.encode_utf16().take(i16::MAX as usize).collect();
        self.i16(units.len() as i16);
        for unit in units {
            self.0.extend_from_slice(&unit.to_be_bytes());
        }
    }

    fn stack(&mut self, stack: Option<WireStack>) {
        match stack {
            Some((id, count, damage)) => {
                self.i16(id);
                self.u8(count as u8);
                self.i16(damage);
            }
            None => self.i16(-1),
        }
    }

    /// `Packet0KeepAlive`.
    pub fn keep_alive(&mut self) {
        self.u8(0);
    }

    /// `Packet1Login`, the server's answer: the player's entity id, the
    /// world's seed and the dimension they are in.
    pub fn login(&mut self, entity: i32, seed: i64, dimension: i8) {
        self.u8(1);
        self.i32(entity);
        self.string("");
        self.i64(seed);
        self.u8(dimension as u8);
    }

    /// `Packet2Handshake`. `"-"` tells the client not to check the name with
    /// minecraft.net.
    pub fn handshake(&mut self, hash: &str) {
        self.u8(2);
        self.string(hash);
    }

    /// `Packet3Chat`. The client allows 119 characters.
    pub fn chat(&mut self, message: &str) {
        self.u8(3);
        let short: String = message.chars().take(119).collect();
        self.string(&short);
    }

    /// `Packet4UpdateTime`.
    pub fn time(&mut self, time: i64) {
        self.u8(4);
        self.i64(time);
    }

    /// `Packet6SpawnPosition`: where the compass points.
    pub fn spawn_position(&mut self, x: i32, y: i32, z: i32) {
        self.u8(6);
        self.i32(x);
        self.i32(y);
        self.i32(z);
    }

    /// `Packet8UpdateHealth`.
    pub fn health(&mut self, health: i16) {
        self.u8(8);
        self.i16(health);
    }

    /// `Packet9Respawn`.
    pub fn respawn(&mut self, dimension: i8) {
        self.u8(9);
        self.u8(dimension as u8);
    }

    /// `Packet13PlayerLookMove` as `NetServerHandler.teleportTo` sends it:
    /// the eyes go where the client reads `y` and the feet where it reads
    /// `stance`.
    pub fn position(&mut self, x: f64, feet: f64, z: f64, yaw: f32, pitch: f32) {
        self.u8(13);
        self.0.extend_from_slice(&x.to_be_bytes());
        self.0
            .extend_from_slice(&(feet + 1.620_000_004_768_371_6).to_be_bytes());
        self.0.extend_from_slice(&feet.to_be_bytes());
        self.0.extend_from_slice(&z.to_be_bytes());
        self.0.extend_from_slice(&yaw.to_be_bytes());
        self.0.extend_from_slice(&pitch.to_be_bytes());
        self.u8(0);
    }

    fn angle(&mut self, degrees: f32) {
        // `(byte)((int)(rotation * 256.0F / 360.0F))`.
        self.u8((degrees * 256.0 / 360.0) as i32 as u8);
    }

    fn fixed_position(&mut self, feet: [f64; 3]) {
        for value in feet {
            self.i32((value * 32.0).floor() as i32);
        }
    }

    /// `Packet18Animation` with `animate` 1: an entity swings its arm.
    pub fn swing(&mut self, entity: i32) {
        self.u8(18);
        self.i32(entity);
        self.u8(1);
    }

    /// `Packet20NamedEntitySpawn`: another player comes into view. `held`
    /// is the id of the item in their hand, or 0.
    pub fn named_entity_spawn(
        &mut self,
        entity: i32,
        name: &str,
        feet: [f64; 3],
        yaw: f32,
        pitch: f32,
        held: i16,
    ) {
        self.u8(20);
        self.i32(entity);
        self.string(name);
        self.fixed_position(feet);
        self.angle(yaw);
        self.angle(pitch);
        self.i16(held);
    }

    /// `Packet29DestroyEntity`.
    pub fn destroy_entity(&mut self, entity: i32) {
        self.u8(29);
        self.i32(entity);
    }

    /// `Packet34EntityTeleport`.
    pub fn entity_teleport(&mut self, entity: i32, feet: [f64; 3], yaw: f32, pitch: f32) {
        self.u8(34);
        self.i32(entity);
        self.fixed_position(feet);
        self.angle(yaw);
        self.angle(pitch);
    }

    /// `Packet50PreChunk`: the client makes room for a chunk, or drops it.
    pub fn pre_chunk(&mut self, chunk_x: i32, chunk_z: i32, load: bool) {
        self.u8(50);
        self.i32(chunk_x);
        self.i32(chunk_z);
        self.u8(u8::from(load));
    }

    /// `Packet51MapChunk` for a whole chunk. `deflated` is its
    /// `getChunkData` bytes through zlib.
    pub fn map_chunk(&mut self, chunk_x: i32, chunk_z: i32, deflated: &[u8]) {
        self.u8(51);
        self.i32(chunk_x * 16);
        self.i16(0);
        self.i32(chunk_z * 16);
        self.u8(15);
        self.u8(127);
        self.u8(15);
        self.i32(deflated.len() as i32);
        self.0.extend_from_slice(deflated);
    }

    /// `Packet53BlockChange`.
    pub fn block_change(&mut self, x: i32, y: i32, z: i32, block: u8, metadata: u8) {
        self.u8(53);
        self.i32(x);
        self.u8(y as u8);
        self.i32(z);
        self.u8(block);
        self.u8(metadata);
    }

    /// `Packet104WindowItems`.
    pub fn window_items(&mut self, window: i8, slots: &[Option<WireStack>]) {
        self.u8(104);
        self.u8(window as u8);
        self.i16(slots.len() as i16);
        for slot in slots {
            self.stack(*slot);
        }
    }

    fn f32(&mut self, value: f32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn f64(&mut self, value: f64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    /// Append packets written elsewhere.
    pub fn extend(&mut self, other: &Writer) {
        self.0.extend_from_slice(&other.0);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// A velocity component as `Packet28EntityVelocity` and the spawn packets
    /// carry it: blocks per tick, held to 3.9 and scaled by 8000.
    fn motion(&mut self, blocks_per_tick: f32) {
        self.i16((f64::from(blocks_per_tick).clamp(-3.9, 3.9) * 8000.0) as i16);
    }

    /// `Packet5PlayerInventory`: what another player holds (slot 0) or wears
    /// (1 the boots to 4 the helmet). `None` empties the slot.
    pub fn equipment(&mut self, entity: i32, slot: i16, item: Option<(i16, i16)>) {
        self.u8(5);
        self.i32(entity);
        self.i16(slot);
        let (id, damage) = item.unwrap_or((-1, 0));
        self.i16(id);
        self.i16(damage);
    }

    /// `Packet17Sleep`: a player lies down in the bed at this block.
    pub fn sleep(&mut self, entity: i32, x: i32, y: i32, z: i32) {
        self.u8(17);
        self.i32(entity);
        self.u8(0);
        self.i32(x);
        self.u8(y as u8);
        self.i32(z);
    }

    /// `Packet18Animation`: 1 swings the arm, 3 leaves a bed.
    pub fn animation(&mut self, entity: i32, animate: u8) {
        self.u8(18);
        self.i32(entity);
        self.u8(animate);
    }

    /// `Packet21PickupSpawn`: an item lying in the world.
    pub fn pickup_spawn(&mut self, entity: i32, stack: WireStack, at: [f64; 3], motion: Vec3) {
        self.u8(21);
        self.i32(entity);
        self.i16(stack.0);
        self.u8(stack.1 as u8);
        self.i16(stack.2);
        self.fixed_position(at);
        for value in motion.to_array() {
            self.u8((f64::from(value) * 128.0) as i32 as u8);
        }
    }

    /// `Packet22Collect`: `collector` picks `collected` up.
    pub fn collect(&mut self, collected: i32, collector: i32) {
        self.u8(22);
        self.i32(collected);
        self.i32(collector);
    }

    /// `Packet23VehicleSpawn`: a boat (1), cart (10 to 12), primed TNT (50),
    /// arrow (60), snowball (61), egg (62), fireball (63), falling sand (70)
    /// or gravel (71), or bobber (90). `thrown` is the id of whoever let it
    /// go and the velocity it left with, which only some kinds carry.
    pub fn vehicle_spawn(
        &mut self,
        entity: i32,
        kind: i8,
        at: [f64; 3],
        thrown: Option<(i32, Vec3)>,
    ) {
        self.u8(23);
        self.i32(entity);
        self.u8(kind as u8);
        self.fixed_position(at);
        match thrown {
            // The client reads the velocity only after an id above zero.
            Some((owner, motion)) if owner > 0 => {
                self.i32(owner);
                for value in motion.to_array() {
                    self.motion(value);
                }
            }
            _ => self.i32(0),
        }
    }

    /// `Packet24MobSpawn`. `metadata` is the mob's `DataWatcher`, without
    /// its end marker.
    pub fn mob_spawn(
        &mut self,
        entity: i32,
        kind: i8,
        at: [f64; 3],
        yaw: f32,
        pitch: f32,
        metadata: &[u8],
    ) {
        self.u8(24);
        self.i32(entity);
        self.u8(kind as u8);
        self.fixed_position(at);
        self.angle(yaw);
        self.angle(pitch);
        self.0.extend_from_slice(metadata);
        self.u8(127);
    }

    /// `Packet28EntityVelocity`, in blocks per tick.
    pub fn velocity(&mut self, entity: i32, motion: Vec3) {
        self.u8(28);
        self.i32(entity);
        for value in motion.to_array() {
            self.motion(value);
        }
    }

    /// `Packet31RelEntityMove`, `Packet32EntityLook` or
    /// `Packet33RelEntityMoveLook`, whichever carries what is given. The
    /// move is in thirty-seconds of a block and the look in 256ths of a turn.
    pub fn entity_step(&mut self, entity: i32, moved: Option<[i8; 3]>, look: Option<[u8; 2]>) {
        let id = match (moved, look) {
            (Some(_), Some(_)) => 33,
            (Some(_), None) => 31,
            (None, Some(_)) => 32,
            (None, None) => return,
        };
        self.u8(id);
        self.i32(entity);
        for value in moved.into_iter().flatten() {
            self.u8(value as u8);
        }
        for value in look.into_iter().flatten() {
            self.u8(value);
        }
    }

    /// `Packet34EntityTeleport` from already encoded parts.
    pub fn entity_teleport_fixed(&mut self, entity: i32, at: [i32; 3], look: [u8; 2]) {
        self.u8(34);
        self.i32(entity);
        for value in at {
            self.i32(value);
        }
        self.u8(look[0]);
        self.u8(look[1]);
    }

    /// `Packet38EntityStatus`: 2 is hurt, 3 dead.
    pub fn entity_status(&mut self, entity: i32, status: u8) {
        self.u8(38);
        self.i32(entity);
        self.u8(status);
    }

    /// `Packet39AttachEntity`: `entity` rides `vehicle`, or nothing at -1.
    pub fn attach(&mut self, entity: i32, vehicle: i32) {
        self.u8(39);
        self.i32(entity);
        self.i32(vehicle);
    }

    /// `Packet40EntityMetadata`. `metadata` has no end marker.
    pub fn entity_metadata(&mut self, entity: i32, metadata: &[u8]) {
        self.u8(40);
        self.i32(entity);
        self.0.extend_from_slice(metadata);
        self.u8(127);
    }

    /// `Packet52MultiBlockChange`: several blocks of one chunk, each as its
    /// position within the chunk, its id and its metadata.
    pub fn multi_block_change(&mut self, chunk_x: i32, chunk_z: i32, blocks: &[([u8; 3], u8, u8)]) {
        self.u8(52);
        self.i32(chunk_x);
        self.i32(chunk_z);
        self.i16(blocks.len() as i16);
        for ([x, y, z], _, _) in blocks {
            self.i16(((u16::from(*x) << 12) | (u16::from(*z) << 8) | u16::from(*y)) as i16);
        }
        for (_, block, _) in blocks {
            self.u8(*block);
        }
        for (_, _, metadata) in blocks {
            self.u8(*metadata);
        }
    }

    /// `Packet54PlayNoteBlock`.
    pub fn note(&mut self, x: i32, y: i32, z: i32, instrument: u8, pitch: u8) {
        self.u8(54);
        self.i32(x);
        self.i16(y as i16);
        self.i32(z);
        self.u8(instrument);
        self.u8(pitch);
    }

    /// `Packet60Explosion`. Cells further than a byte from the centre are
    /// left out; they still arrive as block changes.
    pub fn explosion(&mut self, center: [f64; 3], size: f32, cells: &[[i32; 3]]) {
        let origin = center.map(|value| value as i32);
        let near: Vec<[i8; 3]> = cells
            .iter()
            .filter_map(|cell| {
                let x = i8::try_from(cell[0] - origin[0]).ok()?;
                let y = i8::try_from(cell[1] - origin[1]).ok()?;
                let z = i8::try_from(cell[2] - origin[2]).ok()?;
                Some([x, y, z])
            })
            .collect();
        self.u8(60);
        for value in center {
            self.f64(value);
        }
        self.f32(size);
        self.i32(near.len() as i32);
        for cell in near {
            for value in cell {
                self.u8(value as u8);
            }
        }
    }

    /// `Packet61DoorChange`, Beta's sound and particle effects: 1003 is a
    /// door, 1000 to 1002 a dispenser's clicks, 2000 its smoke.
    pub fn effect(&mut self, effect: i32, x: i32, y: i32, z: i32, data: i32) {
        self.u8(61);
        self.i32(effect);
        self.i32(x);
        self.u8(y as u8);
        self.i32(z);
        self.i32(data);
    }

    /// `Packet70Bed`: 0 says a bed is missing, 1 starts the rain and 2 ends
    /// it.
    pub fn game_state(&mut self, reason: u8) {
        self.u8(70);
        self.u8(reason);
    }

    /// `Packet71Weather`: a lightning bolt.
    pub fn lightning(&mut self, entity: i32, at: [f64; 3]) {
        self.u8(71);
        self.i32(entity);
        self.u8(1);
        self.fixed_position(at);
    }

    /// `Packet100OpenWindow`: 0 a chest, 1 a workbench, 2 a furnace, 3 a
    /// dispenser. The title is written with `writeUTF`, unlike every other
    /// string.
    pub fn open_window(&mut self, window: i8, kind: u8, title: &str, slots: u8) {
        self.u8(100);
        self.u8(window as u8);
        self.u8(kind);
        self.i16(title.len() as i16);
        self.0.extend_from_slice(title.as_bytes());
        self.u8(slots);
    }

    /// `Packet101CloseWindow`.
    pub fn close_window(&mut self, window: i8) {
        self.u8(101);
        self.u8(window as u8);
    }

    /// `Packet103SetSlot`. Window -1 with slot -1 is the stack on the
    /// cursor.
    pub fn set_slot(&mut self, window: i8, slot: i16, stack: Option<WireStack>) {
        self.u8(103);
        self.u8(window as u8);
        self.i16(slot);
        self.stack(stack);
    }

    /// `Packet105UpdateProgressbar`: a furnace's cook time (0), burn time (1)
    /// or the burn time its fuel started with (2).
    pub fn progress_bar(&mut self, window: i8, bar: i16, value: i16) {
        self.u8(105);
        self.u8(window as u8);
        self.i16(bar);
        self.i16(value);
    }

    /// `Packet106Transaction`: whether a window click was accepted.
    pub fn transaction(&mut self, window: i8, action: i16, accepted: bool) {
        self.u8(106);
        self.u8(window as u8);
        self.i16(action);
        self.u8(u8::from(accepted));
    }

    /// `Packet130UpdateSign`.
    pub fn update_sign(&mut self, x: i32, y: i32, z: i32, lines: &[String; 4]) {
        self.u8(130);
        self.i32(x);
        self.i16(y as i16);
        self.i32(z);
        for line in lines {
            self.string(line);
        }
    }

    /// `Packet255KickDisconnect`.
    pub fn kick(&mut self, reason: &str) {
        self.u8(255);
        self.string(reason);
    }
}
