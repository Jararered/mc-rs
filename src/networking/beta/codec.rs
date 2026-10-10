//! Reading the packets a Beta 1.7.3 client sends and writing the ones it
//! reads. Everything is big-endian, as `DataInputStream` has it, and a string
//! is a short count of UTF-16 code units followed by the units.
//!
//! A packet whose body is a flat run of fields is one entry of
//! [`client_packets!`] or [`server_packets!`], which write its variant and
//! read arm, or its `Writer` method, from that one list. The rest are written
//! out by hand below them.

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

/// A field read the one way its type is written.
trait Get: Sized {
    fn get(reader: &mut Reader) -> Result<Self, ReadError>;
}

/// A field written the one way its type is written.
trait Put {
    fn put(self, writer: &mut Writer);
}

macro_rules! numbers {
    ($($type:ty),*) => {$(
        impl Get for $type {
            fn get(reader: &mut Reader) -> Result<Self, ReadError> {
                Ok(Self::from_be_bytes(reader.take()?))
            }
        }

        impl Put for $type {
            fn put(self, writer: &mut Writer) {
                writer.0.extend_from_slice(&self.to_be_bytes());
            }
        }
    )*};
}

numbers!(u8, i8, i16, i32, i64, f32, f64);

impl Get for bool {
    fn get(reader: &mut Reader) -> Result<Self, ReadError> {
        Ok(reader.u8()? != 0)
    }
}

impl Put for bool {
    fn put(self, writer: &mut Writer) {
        writer.u8(u8::from(self));
    }
}

impl<T: Get + Copy + Default, const N: usize> Get for [T; N] {
    fn get(reader: &mut Reader) -> Result<Self, ReadError> {
        let mut values = [T::default(); N];
        for value in &mut values {
            *value = T::get(reader)?;
        }
        Ok(values)
    }
}

impl<T: Put, const N: usize> Put for [T; N] {
    fn put(self, writer: &mut Writer) {
        for value in self {
            value.put(writer);
        }
    }
}

/// `Packet.writeString`.
impl Put for &str {
    fn put(self, writer: &mut Writer) {
        let units: Vec<u16> = self.encode_utf16().take(i16::MAX as usize).collect();
        writer.i16(units.len() as i16);
        for unit in units {
            unit.to_be_bytes().put(writer);
        }
    }
}

impl Get for Option<WireStack> {
    fn get(reader: &mut Reader) -> Result<Self, ReadError> {
        let id = reader.i16()?;
        if id < 0 {
            return Ok(None);
        }
        Ok(Some((id, reader.i8()?, reader.i16()?)))
    }
}

impl Put for Option<WireStack> {
    fn put(self, writer: &mut Writer) {
        match self {
            Some(stack) => stack.put(writer),
            None => writer.i16(-1),
        }
    }
}

impl Put for WireStack {
    fn put(self, writer: &mut Writer) {
        writer.i16(self.0);
        self.1.put(writer);
        writer.i16(self.2);
    }
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
        u8::get(self)
    }

    fn i8(&mut self) -> Result<i8, ReadError> {
        i8::get(self)
    }

    fn i16(&mut self) -> Result<i16, ReadError> {
        i16::get(self)
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

    /// A height sent as one byte.
    fn byte(&mut self) -> Result<i32, ReadError> {
        Ok(i32::from(self.u8()?))
    }

    /// A height sent as a short.
    fn short(&mut self) -> Result<i32, ReadError> {
        Ok(i32::from(self.i16()?))
    }

    fn sign_lines(&mut self) -> Result<[String; 4], ReadError> {
        Ok([
            self.string(15)?,
            self.string(15)?,
            self.string(15)?,
            self.string(15)?,
        ])
    }
}

/// Read one field: by its type, or by the `Reader` method named after `as`.
macro_rules! get {
    ($reader:ident, $type:ty) => {
        <$type as Get>::get($reader)?
    };
    ($reader:ident, $type:ty, $method:ident $(($argument:expr))?) => {
        $reader.$method($($argument)?)?
    };
}

/// The packets a client sends: the [`ClientPacket`] enum and the reading of
/// each variant, from one list. An entry is the packet's id and its variant;
/// a field is read by its type, or `as` a `Reader` method where the wire
/// differs, and a bracketed list of types is read and dropped where it
/// stands. `other` holds the variants [`read_packet`] reads itself.
macro_rules! client_packets {
    (
        unit { $($(#[$unit_doc:meta])* $unit_id:literal => $unit:ident $([$($unit_skip:ty),+])?,)* }
        tuple { $($(#[$tuple_doc:meta])* $tuple_id:literal => $tuple:ident($tuple_type:ty $(as $tuple_method:ident $(($tuple_argument:expr))?)?),)* }
        fields {$(
            $(#[$doc:meta])*
            $id:literal => $variant:ident {
                $(
                    $(#[$field_doc:meta])*
                    $([$($skip:ty),+])?
                    $field:ident: $type:ty $(as $method:ident $(($argument:expr))?)?
                ),* $(,)?
            } $([$($tail:ty),+])?,
        )*}
        other { $($other:tt)* }
    ) => {
        /// A packet from the client. Fields the server has no use for are
        /// read and dropped.
        #[derive(Debug, Clone, PartialEq)]
        pub enum ClientPacket {
            $($(#[$unit_doc])* $unit,)*
            $($(#[$tuple_doc])* $tuple($tuple_type),)*
            $($(#[$doc])* $variant { $($(#[$field_doc])* $field: $type,)* },)*
            $($other)*
        }

        /// Read the body of a listed packet.
        fn read_listed(id: u8, reader: &mut Reader) -> Result<ClientPacket, ReadError> {
            Ok(match id {
                $($unit_id => {
                    $($(<$unit_skip as Get>::get(reader)?;)+)?
                    ClientPacket::$unit
                })*
                $($tuple_id => ClientPacket::$tuple(
                    get!(reader, $tuple_type $(, $tuple_method $(($tuple_argument))?)?)
                ),)*
                $($id => {
                    $(
                        $($(<$skip as Get>::get(reader)?;)+)?
                        let $field = get!(reader, $type $(, $method $(($argument))?)?);
                    )*
                    $($(<$tail as Get>::get(reader)?;)+)?
                    ClientPacket::$variant { $($field),* }
                })*
                other => return Err(ReadError::Invalid(format!("packet id {other}"))),
            })
        }
    };
}

client_packets! {
    unit {
        0 => KeepAlive,
        9 => Respawn [i8],
        18 => Animation [i32, i8],
        27 => VehicleInput [[u8; 18]],
    }
    tuple {
        3 => Chat(String as string(119)),
        16 => HeldSlot(i16),
        101 => CloseWindow(i8),
        255 => Disconnect(String as string(100)),
    }
    fields {
        1 => Login { protocol: i32, username: String as string(16) } [i64, i8],
        2 => Handshake { username: String as string(32) },
        7 => UseEntity { [i32] target: i32, attack: bool },
        14 => Dig { status: u8, x: i32, y: i32 as byte, z: i32, face: u8 },
        15 => Place { x: i32, y: i32 as byte, z: i32, direction: u8, held: Option<WireStack> },
        19 => EntityAction { [i32] state: i8 },
        /// `Packet102WindowClick`.
        102 => WindowClick {
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
        106 => Transaction { window: i8, action: i16 } [u8],
        /// `Packet130UpdateSign`: what the client wrote on a sign.
        130 => UpdateSign { x: i32, y: i32 as short, z: i32, lines: [String; 4] as sign_lines },
    }
    other {
        /// `Packet10Flying` and its subclasses. A part the packet does not
        /// carry is `None`.
        Move {
            /// `x`, `y` (the feet), `stance` (the eyes), `z`.
            position: Option<[f64; 4]>,
            /// Yaw and pitch, in Beta's degrees.
            look: Option<[f32; 2]>,
            on_ground: bool,
        },
    }
}

/// Read one packet from the front of `bytes`: the packet and how many bytes
/// it took.
pub fn read_packet(bytes: &[u8]) -> Result<(ClientPacket, usize), ReadError> {
    let mut reader = Reader { bytes, at: 0 };
    let r = &mut reader;
    let packet = match r.u8()? {
        id @ 10..=13 => ClientPacket::Move {
            position: if id == 11 || id == 13 {
                Some(Get::get(r)?)
            } else {
                None
            },
            look: if id == 12 || id == 13 {
                Some(Get::get(r)?)
            } else {
                None
            },
            on_ground: bool::get(r)?,
        },
        id => read_listed(id, r)?,
    };
    Ok((packet, reader.at))
}

/// Packets for the client, appended to a byte buffer.
#[derive(Default)]
pub struct Writer(pub Vec<u8>);

/// Write one field: by its type, or by the `Writer` method named after `as`.
macro_rules! put {
    ($writer:ident, $value:expr) => {
        Put::put($value, $writer)
    };
    ($writer:ident, $value:expr, $method:ident) => {
        $writer.$method($value)
    };
}

/// The `Writer` method of each packet whose body is its arguments in order.
/// An argument is written by its type, or `as` a `Writer` method where the
/// wire differs; a bracketed list before an argument, or one after `;`, is
/// constants the packet carries there.
macro_rules! server_packets {
    ($(
        $(#[$doc:meta])*
        $id:literal $name:ident(
            $($([$($before:expr),+])? $field:ident: $type:ty $(as $method:ident)?),*
            $(; $($after:expr),+)?
        );
    )*) => {$(
        $(#[$doc])*
        pub fn $name(&mut self $(, $field: $type)*) {
            self.u8($id);
            $(
                $($(Put::put($before, self);)+)?
                put!(self, $field $(, $method)?);
            )*
            $($(Put::put($after, self);)+)?
        }
    )*};
}

impl Writer {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn i16(&mut self, value: i16) {
        value.put(self);
    }

    fn i32(&mut self, value: i32) {
        value.put(self);
    }

    fn string(&mut self, value: &str) {
        value.put(self);
    }

    /// A height sent as one byte.
    fn byte(&mut self, y: i32) {
        self.u8(y as u8);
    }

    /// A height sent as a short.
    fn short(&mut self, y: i32) {
        self.i16(y as i16);
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

    /// A velocity as `Packet28EntityVelocity` and the spawn packets carry
    /// it: blocks per tick, each part held to 3.9 and scaled by 8000.
    fn motion3(&mut self, motion: Vec3) {
        for blocks_per_tick in motion.to_array() {
            self.i16((f64::from(blocks_per_tick).clamp(-3.9, 3.9) * 8000.0) as i16);
        }
    }

    /// Bytes that are already encoded.
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    /// `DataOutputStream.writeUTF`, for text that is all ASCII.
    fn utf(&mut self, text: &str) {
        self.i16(text.len() as i16);
        self.bytes(text.as_bytes());
    }

    /// Append packets written elsewhere.
    pub fn extend(&mut self, other: &Writer) {
        self.bytes(&other.0);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    server_packets! {
        /// `Packet0KeepAlive`.
        0 keep_alive();
        /// `Packet1Login`, the server's answer: the player's entity id, the
        /// world's seed and the dimension they are in.
        1 login(entity: i32, [""] seed: i64, dimension: i8);
        /// `Packet2Handshake`. `"-"` tells the client not to check the name
        /// with minecraft.net.
        2 handshake(hash: &str);
        /// `Packet4UpdateTime`.
        4 time(time: i64);
        /// `Packet6SpawnPosition`: where the compass points.
        6 spawn_position(x: i32, y: i32, z: i32);
        /// `Packet8UpdateHealth`.
        8 health(health: i16);
        /// `Packet9Respawn`.
        9 respawn(dimension: i8);
        /// `Packet17Sleep`: a player lies down in the bed at this block.
        17 sleep(entity: i32, [0u8] x: i32, y: i32 as byte, z: i32);
        /// `Packet18Animation` with `animate` 1: an entity swings its arm.
        18 swing(entity: i32; 1u8);
        /// `Packet18Animation`: 1 swings the arm, 3 leaves a bed.
        18 animation(entity: i32, animate: u8);
        /// `Packet20NamedEntitySpawn`: another player comes into view. `held`
        /// is the id of the item in their hand, or 0.
        20 named_entity_spawn(
            entity: i32,
            name: &str,
            feet: [f64; 3] as fixed_position,
            yaw: f32 as angle,
            pitch: f32 as angle,
            held: i16
        );
        /// `Packet22Collect`: `collector` picks `collected` up.
        22 collect(collected: i32, collector: i32);
        /// `Packet24MobSpawn`. `metadata` is the mob's `DataWatcher`, without
        /// its end marker.
        24 mob_spawn(
            entity: i32,
            kind: i8,
            at: [f64; 3] as fixed_position,
            yaw: f32 as angle,
            pitch: f32 as angle,
            metadata: &[u8] as bytes;
            127u8
        );
        /// `Packet28EntityVelocity`, in blocks per tick.
        28 velocity(entity: i32, motion: Vec3 as motion3);
        /// `Packet29DestroyEntity`.
        29 destroy_entity(entity: i32);
        /// `Packet34EntityTeleport`.
        34 entity_teleport(
            entity: i32,
            feet: [f64; 3] as fixed_position,
            yaw: f32 as angle,
            pitch: f32 as angle
        );
        /// `Packet34EntityTeleport` from already encoded parts.
        34 entity_teleport_fixed(entity: i32, at: [i32; 3], look: [u8; 2]);
        /// `Packet38EntityStatus`: 2 is hurt, 3 dead.
        38 entity_status(entity: i32, status: u8);
        /// `Packet39AttachEntity`: `entity` rides `vehicle`, or nothing at -1.
        39 attach(entity: i32, vehicle: i32);
        /// `Packet40EntityMetadata`. `metadata` has no end marker.
        40 entity_metadata(entity: i32, metadata: &[u8] as bytes; 127u8);
        /// `Packet50PreChunk`: the client makes room for a chunk, or drops it.
        50 pre_chunk(chunk_x: i32, chunk_z: i32, load: bool);
        /// `Packet53BlockChange`.
        53 block_change(x: i32, y: i32 as byte, z: i32, block: u8, metadata: u8);
        /// `Packet54PlayNoteBlock`.
        54 note(x: i32, y: i32 as short, z: i32, instrument: u8, pitch: u8);
        /// `Packet61DoorChange`, Beta's sound and particle effects: 1003 is a
        /// door, 1000 to 1002 a dispenser's clicks, 2000 its smoke.
        61 effect(effect: i32, x: i32, y: i32 as byte, z: i32, data: i32);
        /// `Packet70Bed`: 0 says a bed is missing, 1 starts the rain and 2
        /// ends it.
        70 game_state(reason: u8);
        /// `Packet71Weather`: a lightning bolt.
        71 lightning(entity: i32, [1u8] at: [f64; 3] as fixed_position);
        /// `Packet100OpenWindow`: 0 a chest, 1 a workbench, 2 a furnace, 3 a
        /// dispenser. The title is written with `writeUTF`, unlike every
        /// other string.
        100 open_window(window: i8, kind: u8, title: &str as utf, slots: u8);
        /// `Packet101CloseWindow`.
        101 close_window(window: i8);
        /// `Packet103SetSlot`. Window -1 with slot -1 is the stack on the
        /// cursor.
        103 set_slot(window: i8, slot: i16, stack: Option<WireStack>);
        /// `Packet105UpdateProgressbar`: a furnace's cook time (0), burn time
        /// (1) or the burn time its fuel started with (2).
        105 progress_bar(window: i8, bar: i16, value: i16);
        /// `Packet106Transaction`: whether a window click was accepted.
        106 transaction(window: i8, action: i16, accepted: bool);
        /// `Packet255KickDisconnect`.
        255 kick(reason: &str);
    }

    /// `Packet3Chat`. The client allows 119 characters.
    pub fn chat(&mut self, message: &str) {
        self.u8(3);
        let short: String = message.chars().take(119).collect();
        self.string(&short);
    }

    /// `Packet13PlayerLookMove` as `NetServerHandler.teleportTo` sends it:
    /// the eyes go where the client reads `y` and the feet where it reads
    /// `stance`.
    pub fn position(&mut self, x: f64, feet: f64, z: f64, yaw: f32, pitch: f32) {
        self.u8(13);
        [x, feet + 1.620_000_004_768_371_6, feet, z].put(self);
        [yaw, pitch].put(self);
        self.u8(0);
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
                self.motion3(motion);
            }
            _ => self.i32(0),
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
        center.put(self);
        size.put(self);
        self.i32(near.len() as i32);
        for cell in near {
            for value in cell {
                self.u8(value as u8);
            }
        }
    }

    /// `Packet104WindowItems`.
    pub fn window_items(&mut self, window: i8, slots: &[Option<WireStack>]) {
        self.u8(104);
        self.u8(window as u8);
        self.i16(slots.len() as i16);
        for slot in slots {
            slot.put(self);
        }
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
}
