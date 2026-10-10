//! Reading the packets a Beta 1.7.3 client sends and writing the ones it
//! reads. Everything is big-endian, as `DataInputStream` has it, and a string
//! is a short count of UTF-16 code units followed by the units.

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
    CloseWindow,
    WindowClick,
    Transaction,
    UpdateSign,
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
        101 => {
            r.i8()?;
            ClientPacket::CloseWindow
        }
        102 => {
            r.take::<7>()?;
            r.stack()?;
            ClientPacket::WindowClick
        }
        106 => {
            r.take::<4>()?;
            ClientPacket::Transaction
        }
        130 => {
            r.take::<10>()?;
            for _ in 0..4 {
                r.string(15)?;
            }
            ClientPacket::UpdateSign
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

    /// `Packet255KickDisconnect`.
    pub fn kick(&mut self, reason: &str) {
        self.u8(255);
        self.string(reason);
    }
}
