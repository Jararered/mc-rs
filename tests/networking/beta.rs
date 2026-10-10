//! A Beta 1.7.3 client's side of the wire, played against `BetaServer`.

use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::prelude::*;
use flate2::read::ZlibDecoder;
use game::block::blocks::Block;
use game::entity::combat::Source;
use game::entity::explosion::Explosion;
use game::entity::explosion::prime_tnt;
use game::entity::mobs::MobType;
use game::entity::mobs::SpawnMob;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::Item;
use game::item::ItemStack;
use game::networking::beta::BetaServer;
use game::networking::beta::ServerConfig;
use game::networking::beta::codec::ClientPacket;
use game::networking::beta::codec::ReadError;
use game::networking::beta::codec::read_packet;
use game::player::PlayerHealth;
use game::player::PlayerMovementInput;
use game::world::chunk::WorldChunks;
use game::world::dimension::Dimension;
use game::world::persistence::WorldStorage;

/// What the server sent, as far as these tests look.
#[derive(Debug, Clone, PartialEq)]
enum Heard {
    KeepAlive,
    Login {
        entity: i32,
        dimension: i8,
    },
    Handshake(String),
    Chat(String),
    Time(i64),
    Spawn,
    /// `Packet5PlayerInventory`: what another player holds or wears.
    Equipment {
        entity: i32,
        slot: i16,
        item: i16,
    },
    Health(i16),
    Respawn,
    /// `x`, the eyes, the feet, `z`.
    Position([f64; 4]),
    Sleep(i32),
    Swing(i32),
    /// `Packet18Animation` other than an arm swing.
    Animation {
        entity: i32,
        animate: u8,
    },
    /// `Packet20NamedEntitySpawn`: another player, and where their feet are
    /// in thirty-seconds of a block.
    Player {
        entity: i32,
        name: String,
        at: [i32; 3],
    },
    /// `Packet21PickupSpawn`.
    Item {
        entity: i32,
        stack: (i16, i8, i16),
    },
    Collect {
        collected: i32,
        collector: i32,
    },
    /// `Packet23VehicleSpawn`.
    Object {
        entity: i32,
        kind: i8,
    },
    /// `Packet24MobSpawn`, with its metadata as index and byte pairs.
    Mob {
        entity: i32,
        kind: i8,
        at: [i32; 3],
        bytes: Vec<(u8, u8)>,
    },
    Velocity(i32),
    Gone(i32),
    /// `Packet31RelEntityMove`, `Packet32EntityLook` or both at once.
    Step {
        entity: i32,
        moved: Option<[i8; 3]>,
    },
    Moved {
        entity: i32,
        at: [i32; 3],
    },
    Status {
        entity: i32,
        status: u8,
    },
    Attach {
        entity: i32,
        vehicle: i32,
    },
    Metadata {
        entity: i32,
        bytes: Vec<(u8, u8)>,
    },
    PreChunk {
        x: i32,
        z: i32,
        load: bool,
    },
    /// The chunk's block coordinates and how long its data inflates to.
    MapChunk {
        x: i32,
        z: i32,
        inflated: usize,
    },
    /// `Packet52MultiBlockChange`: each block's world position and id.
    MultiBlock(Vec<([i32; 3], u8)>),
    BlockChange {
        at: [i32; 3],
        block: u8,
    },
    Note,
    /// How many blocks it took.
    Explosion(i32),
    Effect {
        effect: i32,
        at: [i32; 3],
    },
    GameState(u8),
    Lightning,
    OpenWindow {
        window: i8,
        kind: u8,
        title: String,
        slots: u8,
    },
    CloseWindow(i8),
    SetSlot {
        window: i8,
        slot: i16,
        stack: Option<(i16, i8, i16)>,
    },
    WindowItems {
        window: i8,
        slots: Vec<Option<(i16, i8, i16)>>,
    },
    Progress,
    Transaction {
        window: i8,
        action: i16,
        accepted: bool,
    },
    /// `Packet130UpdateSign`.
    Sign {
        at: [i32; 3],
        lines: [String; 4],
    },
    Kick(String),
}

struct Wire<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Wire<'_> {
    fn take(&mut self, count: usize) -> Option<&[u8]> {
        let slice = self.bytes.get(self.at..self.at + count)?;
        self.at += count;
        Some(slice)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn string(&mut self) -> Option<String> {
        let length = self.i16()? as usize;
        let units: Vec<u16> = self
            .take(length * 2)?
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        Some(String::from_utf16_lossy(&units))
    }
    fn stack(&mut self) -> Option<Option<(i16, i8, i16)>> {
        let id = self.i16()?;
        Some(if id < 0 {
            None
        } else {
            Some((id, self.u8()? as i8, self.i16()?))
        })
    }
    /// `DataWatcher.readWatchableObjects`, keeping the byte values.
    fn metadata(&mut self) -> Option<Vec<(u8, u8)>> {
        let mut bytes = Vec::new();
        loop {
            let header = self.u8()?;
            if header == 127 {
                return Some(bytes);
            }
            match header >> 5 {
                0 => bytes.push((header & 31, self.u8()?)),
                1 => drop(self.take(2)?),
                2 | 3 => drop(self.take(4)?),
                4 => drop(self.string()?),
                5 => drop(self.take(5)?),
                6 => drop(self.take(12)?),
                other => panic!("metadata of type {other}"),
            }
        }
    }
}

/// One server packet from the front of `bytes`, or `None` until it is whole.
#[allow(clippy::too_many_lines)]
fn hear(bytes: &[u8]) -> Option<(Heard, usize)> {
    let mut wire = Wire { bytes, at: 0 };
    let w = &mut wire;
    let heard = match w.u8()? {
        0 => Heard::KeepAlive,
        1 => {
            let entity = w.i32()?;
            w.string()?;
            w.i64()?;
            Heard::Login {
                entity,
                dimension: w.u8()? as i8,
            }
        }
        2 => Heard::Handshake(w.string()?),
        3 => Heard::Chat(w.string()?),
        4 => Heard::Time(w.i64()?),
        5 => {
            let entity = w.i32()?;
            let slot = w.i16()?;
            let item = w.i16()?;
            w.i16()?;
            Heard::Equipment { entity, slot, item }
        }
        6 => {
            w.take(12)?;
            Heard::Spawn
        }
        8 => Heard::Health(w.i16()?),
        9 => {
            w.u8()?;
            Heard::Respawn
        }
        13 => {
            let position = [w.f64()?, w.f64()?, w.f64()?, w.f64()?];
            w.take(9)?;
            Heard::Position(position)
        }
        17 => {
            let entity = w.i32()?;
            w.take(10)?;
            Heard::Sleep(entity)
        }
        18 => {
            let entity = w.i32()?;
            match w.u8()? {
                1 => Heard::Swing(entity),
                animate => Heard::Animation { entity, animate },
            }
        }
        20 => {
            let entity = w.i32()?;
            let name = w.string()?;
            let at = [w.i32()?, w.i32()?, w.i32()?];
            w.take(4)?;
            Heard::Player { entity, name, at }
        }
        21 => {
            let entity = w.i32()?;
            let stack = (w.i16()?, w.u8()? as i8, w.i16()?);
            w.take(15)?;
            Heard::Item { entity, stack }
        }
        22 => Heard::Collect {
            collected: w.i32()?,
            collector: w.i32()?,
        },
        23 => {
            let entity = w.i32()?;
            let kind = w.u8()? as i8;
            w.take(12)?;
            if w.i32()? > 0 {
                w.take(6)?;
            }
            Heard::Object { entity, kind }
        }
        24 => {
            let entity = w.i32()?;
            let kind = w.u8()? as i8;
            let at = [w.i32()?, w.i32()?, w.i32()?];
            w.take(2)?;
            Heard::Mob {
                entity,
                kind,
                at,
                bytes: w.metadata()?,
            }
        }
        28 => {
            let entity = w.i32()?;
            w.take(6)?;
            Heard::Velocity(entity)
        }
        29 => Heard::Gone(w.i32()?),
        id @ 31..=33 => {
            let entity = w.i32()?;
            let moved = if id == 32 {
                None
            } else {
                Some([w.u8()? as i8, w.u8()? as i8, w.u8()? as i8])
            };
            if id != 31 {
                w.take(2)?;
            }
            Heard::Step { entity, moved }
        }
        34 => {
            let entity = w.i32()?;
            let at = [w.i32()?, w.i32()?, w.i32()?];
            w.take(2)?;
            Heard::Moved { entity, at }
        }
        38 => Heard::Status {
            entity: w.i32()?,
            status: w.u8()?,
        },
        39 => Heard::Attach {
            entity: w.i32()?,
            vehicle: w.i32()?,
        },
        40 => Heard::Metadata {
            entity: w.i32()?,
            bytes: w.metadata()?,
        },
        50 => Heard::PreChunk {
            x: w.i32()?,
            z: w.i32()?,
            load: w.u8()? != 0,
        },
        51 => {
            let x = w.i32()?;
            assert_eq!(w.i16()?, 0);
            let z = w.i32()?;
            assert_eq!(w.take(3)?, [15, 127, 15]);
            let length = w.i32()? as usize;
            let mut inflated = Vec::new();
            ZlibDecoder::new(w.take(length)?)
                .read_to_end(&mut inflated)
                .unwrap();
            Heard::MapChunk {
                x,
                z,
                inflated: inflated.len(),
            }
        }
        52 => {
            let chunk = [w.i32()?, w.i32()?];
            let count = w.i16()? as usize;
            let mut cells = Vec::new();
            for _ in 0..count {
                let packed = w.i16()? as u16;
                cells.push([
                    chunk[0] * 16 + i32::from(packed >> 12),
                    i32::from(packed & 255),
                    chunk[1] * 16 + i32::from((packed >> 8) & 15),
                ]);
            }
            let blocks = w.take(count)?.to_vec();
            w.take(count)?;
            Heard::MultiBlock(cells.into_iter().zip(blocks).collect())
        }
        53 => {
            let x = w.i32()?;
            let y = i32::from(w.u8()?);
            let z = w.i32()?;
            let block = w.u8()?;
            w.u8()?;
            Heard::BlockChange {
                at: [x, y, z],
                block,
            }
        }
        54 => {
            w.take(12)?;
            Heard::Note
        }
        60 => {
            w.take(28)?;
            let count = w.i32()?;
            w.take(count as usize * 3)?;
            Heard::Explosion(count)
        }
        61 => {
            let effect = w.i32()?;
            let x = w.i32()?;
            let y = i32::from(w.u8()?);
            let z = w.i32()?;
            w.i32()?;
            Heard::Effect {
                effect,
                at: [x, y, z],
            }
        }
        70 => Heard::GameState(w.u8()?),
        71 => {
            w.take(17)?;
            Heard::Lightning
        }
        100 => {
            let window = w.u8()? as i8;
            let kind = w.u8()?;
            // `writeUTF`: a byte count, then the bytes.
            let length = w.i16()? as usize;
            let title = String::from_utf8(w.take(length)?.to_vec()).unwrap();
            Heard::OpenWindow {
                window,
                kind,
                title,
                slots: w.u8()?,
            }
        }
        101 => Heard::CloseWindow(w.u8()? as i8),
        103 => Heard::SetSlot {
            window: w.u8()? as i8,
            slot: w.i16()?,
            stack: w.stack()?,
        },
        104 => {
            let window = w.u8()? as i8;
            let count = w.i16()?;
            let mut slots = Vec::new();
            for _ in 0..count {
                slots.push(w.stack()?);
            }
            Heard::WindowItems { window, slots }
        }
        105 => {
            w.take(5)?;
            Heard::Progress
        }
        106 => Heard::Transaction {
            window: w.u8()? as i8,
            action: w.i16()?,
            accepted: w.u8()? != 0,
        },
        130 => {
            let x = w.i32()?;
            let y = i32::from(w.i16()?);
            let z = w.i32()?;
            Heard::Sign {
                at: [x, y, z],
                lines: [w.string()?, w.string()?, w.string()?, w.string()?],
            }
        }
        255 => Heard::Kick(w.string()?),
        other => panic!("the server sent packet {other}, which this client does not read"),
    };
    Some((heard, wire.at))
}

struct Player {
    stream: TcpStream,
    inbox: Vec<u8>,
    heard: Vec<Heard>,
}

fn string(out: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    out.extend((units.len() as i16).to_be_bytes());
    for unit in units {
        out.extend(unit.to_be_bytes());
    }
}

impl Player {
    fn connect(server: &BetaServer) -> Self {
        let stream = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        stream.set_nonblocking(true).unwrap();
        Self {
            stream,
            inbox: Vec::new(),
            heard: Vec::new(),
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.stream.set_nonblocking(false).unwrap();
        self.stream.write_all(bytes).unwrap();
        self.stream.set_nonblocking(true).unwrap();
    }

    fn handshake(&mut self, name: &str) {
        let mut out = vec![2];
        string(&mut out, name);
        self.send(&out);
    }

    fn login(&mut self, name: &str, protocol: i32) {
        let mut out = vec![1];
        out.extend(protocol.to_be_bytes());
        string(&mut out, name);
        out.extend(0i64.to_be_bytes());
        out.push(0);
        self.send(&out);
    }

    /// `Packet13PlayerLookMove`, the way the client writes it: feet, then eyes.
    fn stand(&mut self, x: f64, feet: f64, z: f64) {
        let mut out = vec![13];
        for value in [x, feet, feet + 1.62, z] {
            out.extend(value.to_be_bytes());
        }
        out.extend(0f32.to_be_bytes());
        out.extend(0f32.to_be_bytes());
        out.push(1);
        self.send(&out);
    }

    fn dig(&mut self, status: u8, at: [i32; 3]) {
        let mut out = vec![14, status];
        out.extend(at[0].to_be_bytes());
        out.push(at[1] as u8);
        out.extend(at[2].to_be_bytes());
        out.push(1);
        self.send(&out);
    }

    /// `Packet15Place`: the use button on a block's face, with what is held.
    fn place(&mut self, at: [i32; 3], direction: u8, held: Option<(i16, i8, i16)>) {
        let mut out = vec![15];
        out.extend(at[0].to_be_bytes());
        out.push(at[1] as u8);
        out.extend(at[2].to_be_bytes());
        out.push(direction);
        match held {
            Some((id, count, damage)) => {
                out.extend(id.to_be_bytes());
                out.push(count as u8);
                out.extend(damage.to_be_bytes());
            }
            None => out.extend((-1i16).to_be_bytes()),
        }
        self.send(&out);
    }

    /// `Packet7UseEntity`.
    fn use_entity(&mut self, me: i32, target: i32, attack: bool) {
        let mut out = vec![7];
        out.extend(me.to_be_bytes());
        out.extend(target.to_be_bytes());
        out.push(u8::from(attack));
        self.send(&out);
    }

    /// `Packet19EntityAction`: 1 sneaks, 2 stands up.
    fn action(&mut self, me: i32, state: u8) {
        let mut out = vec![19];
        out.extend(me.to_be_bytes());
        out.push(state);
        self.send(&out);
    }

    /// `Packet102WindowClick` with the left button.
    fn click(
        &mut self,
        window: i8,
        slot: i16,
        action: i16,
        shift: bool,
        clicked: Option<(i16, i8, i16)>,
    ) {
        let mut out = vec![102, window as u8];
        out.extend(slot.to_be_bytes());
        out.push(0);
        out.extend(action.to_be_bytes());
        out.push(u8::from(shift));
        match clicked {
            Some((id, count, damage)) => {
                out.extend(id.to_be_bytes());
                out.push(count as u8);
                out.extend(damage.to_be_bytes());
            }
            None => out.extend((-1i16).to_be_bytes()),
        }
        self.send(&out);
    }

    fn chat(&mut self, text: &str) {
        let mut out = vec![3];
        string(&mut out, text);
        self.send(&out);
    }

    fn read(&mut self) {
        let mut buffer = [0u8; 65536];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => self.inbox.extend_from_slice(&buffer[..count]),
            }
        }
        while let Some((heard, length)) = hear(&self.inbox) {
            self.inbox.drain(..length);
            self.heard.push(heard);
        }
    }
}

fn server(label: &str) -> BetaServer {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let saves = std::env::temp_dir().join(format!("game-beta-server-{label}-{unique}"));
    std::fs::create_dir_all(&saves).unwrap();
    let storage = WorldStorage::create(&saves, 3, "Net").unwrap();
    let config = ServerConfig {
        view_distance: 2,
        ..ServerConfig::default()
    };
    BetaServer::bind("127.0.0.1:0", storage, config).unwrap()
}

/// Tick the server until `player` has heard something `wanted` accepts.
fn until(
    server: &mut BetaServer,
    player: &mut Player,
    what: &str,
    mut wanted: impl FnMut(&Heard) -> bool,
) -> Heard {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut from = 0;
    loop {
        server.tick();
        player.read();
        if let Some(found) = player.heard[from..].iter().find(|heard| wanted(heard)) {
            return found.clone();
        }
        from = player.heard.len();
        if Instant::now() >= deadline {
            // The chunks and the steady traffic say nothing about a failure.
            let recent: Vec<&Heard> = player
                .heard
                .iter()
                .filter(|heard| {
                    !matches!(
                        heard,
                        Heard::KeepAlive
                            | Heard::Time(_)
                            | Heard::PreChunk { .. }
                            | Heard::MapChunk { .. }
                            | Heard::Step { .. }
                            | Heard::Velocity(_)
                            | Heard::Moved { .. }
                            | Heard::Mob { .. }
                            | Heard::Metadata { .. }
                            | Heard::Status { .. }
                    )
                })
                .collect();
            let recent: Vec<String> = recent[recent.len().saturating_sub(40)..]
                .iter()
                .map(|heard| format!("{heard:?}"))
                .collect();
            panic!(
                "never heard {what}; the last it heard was\n{}",
                recent.join("\n")
            );
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn the_codec_reads_what_a_client_writes_and_waits_for_the_rest() {
    let mut bytes = vec![15];
    bytes.extend(7i32.to_be_bytes());
    bytes.push(64);
    bytes.extend((-3i32).to_be_bytes());
    bytes.push(1);
    bytes.extend(4i16.to_be_bytes());
    bytes.push(12);
    bytes.extend(0i16.to_be_bytes());
    // Half a packet is not an error.
    assert_eq!(read_packet(&bytes[..9]), Err(ReadError::Incomplete));
    let (packet, length) = read_packet(&bytes).unwrap();
    assert_eq!(length, bytes.len());
    assert_eq!(
        packet,
        ClientPacket::Place {
            x: 7,
            y: 64,
            z: -3,
            direction: 1,
            held: Some((4, 12, 0)),
        }
    );
    // An empty hand ends at the id.
    let empty = [&bytes[..11], &(-1i16).to_be_bytes()].concat();
    assert_eq!(read_packet(&empty).unwrap().1, 13);
    assert!(matches!(read_packet(&[200]), Err(ReadError::Invalid(_))));
}

#[test]
fn a_beta_client_logs_in_gets_terrain_and_changes_the_world() {
    let mut server = server("play");
    let mut player = Player::connect(&server);
    player.handshake("tester");
    assert_eq!(
        until(&mut server, &mut player, "the handshake", |heard| {
            matches!(heard, Heard::Handshake(_))
        }),
        Heard::Handshake("-".to_owned())
    );

    player.login("tester", 14);
    let Heard::Position([x, eyes, feet, z]) =
        until(&mut server, &mut player, "a position", |heard| {
            matches!(heard, Heard::Position(_))
        })
    else {
        unreachable!()
    };
    assert_eq!(server.players(), ["tester"]);
    assert!(
        player
            .heard
            .iter()
            .any(|heard| matches!(heard, Heard::Login { dimension: 0, .. }))
    );
    // The eyes come where the client reads `y`.
    assert!((eyes - feet - 1.62).abs() < 1e-6, "{eyes} {feet}");
    // The chunk they stand in arrived first, whole: blocks and three nibble
    // arrays.
    let chunk = [(x.floor() as i32) >> 4, (z.floor() as i32) >> 4];
    assert!(player.heard.contains(&Heard::PreChunk {
        x: chunk[0],
        z: chunk[1],
        load: true
    }));
    assert!(player.heard.contains(&Heard::MapChunk {
        x: chunk[0] * 16,
        z: chunk[1] * 16,
        inflated: 16 * 16 * 128 * 5 / 2,
    }));
    until(
        &mut server,
        &mut player,
        "the inventory",
        |heard| matches!(heard, Heard::WindowItems { window: 0, slots } if slots.len() == 45),
    );
    until(&mut server, &mut player, "their health", |heard| {
        *heard == Heard::Health(20)
    });

    // Stood on the surface: the block under the feet is ground. Dig it.
    player.stand(x, feet, z);
    let under = [x.floor() as i32, feet as i32 - 1, z.floor() as i32];
    let ground = server
        .host()
        .world(game::world::dimension::Dimension::Overworld)
        .unwrap()
        .resource::<game::world::chunk::WorldChunks>()
        .block_at(under[0], under[1], under[2]);
    assert!(
        ground.is_some_and(game::block::blocks::Block::is_opaque_cube),
        "{ground:?}"
    );
    player.dig(0, under);
    player.dig(2, under);
    until(&mut server, &mut player, "the block going", |heard| {
        *heard
            == Heard::BlockChange {
                at: under,
                block: 0,
            }
    });

    player.chat("hello");
    until(&mut server, &mut player, "their own chat line", |heard| {
        *heard == Heard::Chat("<tester> hello".to_owned())
    });

    // Leaving saves where they were.
    drop(player);
    for _ in 0..5 {
        server.tick();
    }
    assert!(server.players().is_empty());
    let record = server
        .host()
        .storage()
        .load_named_player("tester")
        .expect("the player's record is saved when they leave");
    assert!((f64::from(record.x) - x).abs() < 0.01);
}

#[test]
fn a_client_of_another_version_is_turned_away() {
    let mut server = server("version");
    let mut player = Player::connect(&server);
    player.handshake("old");
    player.login("old", 13);
    assert_eq!(
        until(&mut server, &mut player, "a kick", |heard| {
            matches!(heard, Heard::Kick(_))
        }),
        Heard::Kick("Outdated client!".to_owned())
    );
    assert!(server.players().is_empty());
}

/// The Overworld the server is running.
fn overworld(server: &mut BetaServer) -> &mut World {
    server
        .host_mut()
        .world_mut(Dimension::Overworld)
        .expect("a player is in the Overworld")
}

/// The id the server gave this client's player.
fn own_id(player: &Player) -> i32 {
    player
        .heard
        .iter()
        .find_map(|heard| match heard {
            Heard::Login { entity, .. } => Some(*entity),
            _ => None,
        })
        .expect("the client logged in")
}

/// Log `name` in and stand where the server says.
fn join(server: &mut BetaServer, name: &str) -> (Player, [f64; 3]) {
    let mut player = Player::connect(server);
    player.handshake(name);
    player.login(name, 14);
    let Heard::Position([x, _, feet, z]) = until(server, &mut player, "a position", |heard| {
        matches!(heard, Heard::Position(_))
    }) else {
        unreachable!()
    };
    player.stand(x, feet, z);
    (player, [x, feet, z])
}

#[test]
fn players_see_each_other_arrive_move_swing_and_leave() {
    let mut server = server("pair");
    let (mut alice, _) = join(&mut server, "alice");
    let (mut bob, [x, feet, z]) = join(&mut server, "bob");

    let fixed = |value: f64| (value * 32.0).floor() as i32;
    let Heard::Player { entity, at, .. } = until(
        &mut server,
        &mut alice,
        "bob arriving",
        |heard| matches!(heard, Heard::Player { name, .. } if name == "bob"),
    ) else {
        unreachable!()
    };
    assert_eq!(at, [fixed(x), fixed(feet), fixed(z)]);
    until(
        &mut server,
        &mut bob,
        "alice already there",
        |heard| matches!(heard, Heard::Player { name, .. } if name == "alice"),
    );
    // Nobody is told about themselves.
    assert!(
        !bob.heard
            .iter()
            .any(|heard| matches!(heard, Heard::Player { name, .. } if name == "bob"))
    );

    // A step is sent as how far it went, in thirty-seconds of a block.
    bob.stand(x + 1.5, feet, z);
    bob.send(&[&[18u8][..], &entity.to_be_bytes(), &[1]].concat());
    until(&mut server, &mut alice, "bob moving", |heard| {
        *heard
            == Heard::Step {
                entity,
                moved: Some([48, 0, 0]),
            }
    });
    until(&mut server, &mut alice, "bob's swing", |heard| {
        *heard == Heard::Swing(entity)
    });
    assert_eq!(own_id(&bob), entity);

    // Sneaking is the second of `Entity`'s flags.
    bob.action(entity, 1);
    until(&mut server, &mut alice, "bob sneaking", |heard| {
        *heard
            == Heard::Metadata {
                entity,
                bytes: vec![(0, 2)],
            }
    });

    // What he takes in hand is shown to her.
    let (_, body) = server.host_mut().find("bob").unwrap();
    overworld(&mut server)
        .get_mut::<Hotbar>(body)
        .unwrap()
        .slots[0] = ItemStack::new(Item::DiamondSword, 1).ok();
    until(&mut server, &mut alice, "bob's sword", |heard| {
        *heard
            == Heard::Equipment {
                entity,
                slot: 0,
                item: Item::DiamondSword.as_u16() as i16,
            }
    });

    drop(bob);
    until(&mut server, &mut alice, "bob leaving", |heard| {
        *heard == Heard::Gone(entity)
    });
}

#[test]
fn a_client_is_shown_mobs_and_hits_them() {
    let mut server = server("mobs");
    let (mut player, [x, feet, z]) = join(&mut server, "hunter");
    let me = own_id(&player);

    // A slime of a size that does not spawn up here by itself.
    let spot = Vec3::new(x as f32 + 2.0, feet as f32 + 1.0, z as f32);
    overworld(&mut server)
        .resource_mut::<Messages<SpawnMob>>()
        .write(SpawnMob {
            kind: MobType::Slime,
            feet: spot,
            variant: 3,
        });
    let Heard::Mob { entity: slime, .. } = until(
        &mut server,
        &mut player,
        "the slime",
        |heard| matches!(heard, Heard::Mob { kind: 55, bytes, .. } if bytes.contains(&(16, 3))),
    ) else {
        unreachable!()
    };

    // `Packet7UseEntity` with the left button is a blow: the slime flinches.
    player.use_entity(me, slime, true);
    until(&mut server, &mut player, "the slime being hurt", |heard| {
        *heard
            == Heard::Status {
                entity: slime,
                status: 2,
            }
    });
}

#[test]
fn drops_are_collected_and_window_clicks_answered() {
    let mut server = server("items");
    let (mut player, [x, feet, z]) = join(&mut server, "miner");
    let me = own_id(&player);

    // Dig the ground out from underfoot and step down onto what it dropped.
    let under = [x.floor() as i32, feet as i32 - 1, z.floor() as i32];
    player.dig(0, under);
    player.dig(2, under);
    let Heard::Item {
        entity: item,
        stack,
    } = until(&mut server, &mut player, "the drop", |heard| {
        matches!(heard, Heard::Item { .. })
    })
    else {
        unreachable!()
    };
    player.stand(x, feet - 1.0, z);
    until(
        &mut server,
        &mut player,
        "the drop being collected",
        |heard| {
            *heard
                == Heard::Collect {
                    collected: item,
                    collector: me,
                }
        },
    );
    until(
        &mut server,
        &mut player,
        "the drop leaving the world",
        |heard| *heard == Heard::Gone(item),
    );
    // It lands in the first hotbar slot, window slot 36.
    until(
        &mut server,
        &mut player,
        "the drop in the hotbar",
        |heard| {
            *heard
                == Heard::SetSlot {
                    window: 0,
                    slot: 36,
                    stack: Some(stack),
                }
        },
    );

    // Pick it up and put it in the main inventory: both clicks are accepted,
    // and nothing is sent back for what the client did itself.
    player.click(0, 36, 1, false, Some(stack));
    player.click(0, 9, 2, false, None);
    until(
        &mut server,
        &mut player,
        "the second click's answer",
        |heard| {
            *heard
                == Heard::Transaction {
                    window: 0,
                    action: 2,
                    accepted: true,
                }
        },
    );
    assert!(player.heard.contains(&Heard::Transaction {
        window: 0,
        action: 1,
        accepted: true,
    }));
    let (_, body) = server.host_mut().find("miner").unwrap();
    let world = overworld(&mut server);
    assert_eq!(world.get::<Hotbar>(body).unwrap().slots[0], None);
    let moved = world.get::<Inventory>(body).unwrap().main[0].unwrap();
    assert_eq!(
        (moved.item().as_u16() as i16, moved.count() as i8),
        (stack.0, stack.1)
    );

    // A click the client got wrong is refused and its window sent whole.
    let heard = player.heard.len();
    player.click(0, 10, 3, false, Some((1, 1, 0)));
    until(&mut server, &mut player, "the refusal", |heard| {
        *heard
            == Heard::Transaction {
                window: 0,
                action: 3,
                accepted: false,
            }
    });
    until(&mut server, &mut player, "the inventory again", |heard| {
        matches!(heard, Heard::WindowItems { window: 0, .. })
    });
    assert!(player.heard[heard..].iter().any(
        |heard| matches!(heard, Heard::WindowItems { window: 0, slots } if slots[9] == Some(stack))
    ));
}

#[test]
fn a_chest_opens_as_a_window_and_takes_a_shift_click() {
    let mut server = server("chest");
    let (mut player, [x, feet, z]) = join(&mut server, "keeper");
    let (_, body) = server.host_mut().find("keeper").unwrap();
    let cobble = ItemStack::new(Item::from_block(Block::Cobblestone).unwrap(), 5).unwrap();
    let chest = ItemStack::new(Item::from_block(Block::Chest).unwrap(), 1).unwrap();
    let world = overworld(&mut server);
    let mut hotbar = world.get_mut::<Hotbar>(body).unwrap();
    hotbar.slots[0] = Some(chest);
    hotbar.slots[1] = Some(cobble);

    // The ground two blocks along, wherever its surface is.
    let column = [x.floor() as i32 + 2, z.floor() as i32];
    let chunks = world.resource::<WorldChunks>();
    let top = (0..127)
        .rev()
        .find(|y| {
            chunks
                .block_at(column[0], *y, column[1])
                .is_some_and(Block::is_opaque_cube)
        })
        .expect("there is ground beside the spawn");
    assert!(
        (f64::from(top) - feet).abs() < 5.0,
        "the ground is in reach"
    );
    let ground = [column[0], top, column[1]];
    let above = [column[0], top + 1, column[1]];

    player.place(ground, 1, Some((Block::Chest.as_u8().into(), 1, 0)));
    until(&mut server, &mut player, "the chest", |heard| match heard {
        Heard::BlockChange { at, block } => *at == above && *block == Block::Chest.as_u8(),
        Heard::MultiBlock(blocks) => blocks.contains(&(above, Block::Chest.as_u8())),
        _ => false,
    });

    // The use button on it opens `ContainerChest`: 27 slots of its own and
    // the player's 36.
    player.place(above, 1, None);
    let Heard::OpenWindow { window, .. } =
        until(&mut server, &mut player, "the chest opening", |heard| {
            matches!(
                heard,
                Heard::OpenWindow { kind: 0, slots: 27, title, .. } if title == "Chest"
            )
        })
    else {
        unreachable!()
    };
    let wire = (cobble.item().as_u16() as i16, 5, 0);
    until(
        &mut server,
        &mut player,
        "the chest's slots",
        |heard| matches!(heard, Heard::WindowItems { window: id, slots } if *id == window && slots.len() == 63 && slots[55] == Some(wire)),
    );

    // The second hotbar slot is the window's slot 55. Shift sends it across.
    player.click(window, 55, 1, true, Some(wire));
    until(&mut server, &mut player, "the click's answer", |heard| {
        *heard
            == Heard::Transaction {
                window,
                action: 1,
                accepted: true,
            }
    });
    let world = overworld(&mut server);
    assert_eq!(
        world
            .resource::<WorldChunks>()
            .chest_at(above[0], above[1], above[2])
            .expect("the chest has its slots")
            .slots[0],
        Some(cobble)
    );
    assert_eq!(world.get::<Hotbar>(body).unwrap().slots[1], None);
}

#[test]
fn an_explosion_is_heard_and_its_blocks_sent_together() {
    let mut server = server("blast");
    let (mut player, [x, feet, z]) = join(&mut server, "sapper");
    overworld(&mut server)
        .resource_mut::<Messages<Explosion>>()
        .write(Explosion {
            center: Vec3::new(x as f32 + 5.0, feet as f32 - 2.0, z as f32),
            strength: 3.0,
            flaming: false,
            source: Source::Environment,
        });
    let Heard::Explosion(cells) = until(&mut server, &mut player, "the blast", |heard| {
        matches!(heard, Heard::Explosion(_))
    }) else {
        unreachable!()
    };
    assert!(cells > 1, "the blast took {cells} blocks");
    // Several blocks of one chunk go in one packet, all of them now air.
    let Heard::MultiBlock(blocks) = until(&mut server, &mut player, "the hole", |heard| {
        matches!(heard, Heard::MultiBlock(_))
    }) else {
        unreachable!()
    };
    assert!(blocks.len() > 1);
    assert!(blocks.iter().any(|(_, block)| *block == 0));
}

#[test]
fn a_crafting_result_is_taken_through_window_clicks() {
    let mut server = server("craft");
    let (mut player, _) = join(&mut server, "joiner");
    let (_, body) = server.host_mut().find("joiner").unwrap();
    let log = ItemStack::new(Item::from_block(Block::Wood).unwrap(), 1).unwrap();
    overworld(&mut server)
        .get_mut::<Hotbar>(body)
        .unwrap()
        .slots[0] = Some(log);
    let log_wire = (log.item().as_u16() as i16, 1, 0);
    until(&mut server, &mut player, "the log", |heard| {
        *heard
            == Heard::SetSlot {
                window: 0,
                slot: 36,
                stack: Some(log_wire),
            }
    });

    // The log goes from the hotbar into the 2x2 grid, and the planks it
    // makes come out of the result slot onto the cursor.
    let planks = (Block::WoodenPlanks.as_u8().into(), 4, 0);
    player.click(0, 36, 1, false, Some(log_wire));
    player.click(0, 1, 2, false, None);
    player.click(0, 0, 3, false, Some(planks));
    until(&mut server, &mut player, "the planks' click", |heard| {
        *heard
            == Heard::Transaction {
                window: 0,
                action: 3,
                accepted: true,
            }
    });
    let inventory = overworld(&mut server).get::<Inventory>(body).unwrap();
    assert_eq!(inventory.crafting, [None; 4], "the log was used up");
    let carried = inventory.carried.expect("the planks are on the cursor");
    assert_eq!(
        (carried.item().as_u16() as i16, carried.count() as i8),
        (planks.0, planks.1)
    );
}

#[test]
fn a_dead_player_stays_down_until_their_client_respawns() {
    let mut server = server("death");
    let (mut alice, _) = join(&mut server, "alice");
    let (mut bob, _) = join(&mut server, "bob");
    let bob_id = own_id(&bob);
    until(
        &mut server,
        &mut alice,
        "bob arriving",
        |heard| matches!(heard, Heard::Player { name, .. } if name == "bob"),
    );

    let (_, body) = server.host_mut().find("bob").unwrap();
    overworld(&mut server)
        .get_mut::<PlayerHealth>(body)
        .unwrap()
        .current = 0;
    until(&mut server, &mut bob, "his death", |heard| {
        *heard == Heard::Health(0)
    });
    until(&mut server, &mut alice, "bob dying", |heard| {
        *heard
            == Heard::Status {
                entity: bob_id,
                status: 3,
            }
    });
    // Nothing happens while his client shows the death screen.
    for _ in 0..80 {
        server.tick();
    }
    assert_eq!(
        overworld(&mut server)
            .get::<PlayerHealth>(body)
            .unwrap()
            .current,
        0
    );

    // `Packet9Respawn` from the button, answered with the same, then where
    // he now stands and his health.
    let seen = alice.heard.len();
    bob.send(&[9, 0]);
    until(&mut server, &mut bob, "the respawn", |heard| {
        *heard == Heard::Respawn
    });
    for _ in 0..5 {
        server.tick();
        bob.read();
    }
    let respawn = bob
        .heard
        .iter()
        .position(|heard| *heard == Heard::Respawn)
        .unwrap();
    let after = &bob.heard[respawn..];
    assert!(
        after
            .iter()
            .any(|heard| matches!(heard, Heard::Position(_)))
    );
    assert!(after.contains(&Heard::Health(20)));
    // To everyone else the body goes and he arrives again.
    until(
        &mut server,
        &mut alice,
        "bob back",
        |heard| matches!(heard, Heard::Player { entity, .. } if *entity == bob_id),
    );
    assert!(alice.heard[seen..].contains(&Heard::Gone(bob_id)));
}

#[test]
fn a_sign_is_placed_written_on_once_and_read_by_everyone() {
    let mut server = server("sign");
    let (mut writer, [x, feet, z]) = join(&mut server, "writer");
    let (mut reader, _) = join(&mut server, "reader");
    let (_, body) = server.host_mut().find("writer").unwrap();
    overworld(&mut server)
        .get_mut::<Hotbar>(body)
        .unwrap()
        .slots[0] = ItemStack::new(Item::Sign, 1).ok();

    let under = [x.floor() as i32, feet as i32 - 1, z.floor() as i32 + 2];
    let top = (0..127)
        .rev()
        .find(|y| {
            overworld(&mut server)
                .resource::<WorldChunks>()
                .block_at(under[0], *y, under[2])
                .is_some_and(Block::is_opaque_cube)
        })
        .unwrap();
    let post = [under[0], top + 1, under[2]];
    writer.place(
        [under[0], top, under[2]],
        1,
        Some((Item::Sign.as_u16() as i16, 1, 0)),
    );
    until(&mut server, &mut reader, "the sign post", |heard| {
        *heard
            == Heard::BlockChange {
                at: post,
                block: Block::StandingSign.as_u8(),
            }
    });

    // `Packet130UpdateSign` from the editor, relayed to whoever holds the
    // chunk.
    let lines = ["Beware", "of the", "creeper", ""].map(str::to_owned);
    let mut out = vec![130];
    out.extend(post[0].to_be_bytes());
    out.extend((post[1] as i16).to_be_bytes());
    out.extend(post[2].to_be_bytes());
    for line in &lines {
        string(&mut out, line);
    }
    writer.send(&out);
    until(&mut server, &mut reader, "what the sign says", |heard| {
        *heard
            == Heard::Sign {
                at: post,
                lines: lines.clone(),
            }
    });

    // It takes text only once (`isEditable`).
    let mut again = vec![130];
    again.extend(post[0].to_be_bytes());
    again.extend((post[1] as i16).to_be_bytes());
    again.extend(post[2].to_be_bytes());
    for line in ["no", "", "", ""] {
        string(&mut again, line);
    }
    writer.send(&again);
    for _ in 0..5 {
        server.tick();
    }
    assert_eq!(
        overworld(&mut server)
            .resource::<WorldChunks>()
            .sign_at(post[0], post[1], post[2])
            .unwrap()
            .lines,
        lines
    );
}

#[test]
fn one_player_can_hit_another() {
    let mut server = server("pvp");
    let (mut alice, _) = join(&mut server, "alice");
    let (mut bob, _) = join(&mut server, "bob");
    let (alice_id, bob_id) = (own_id(&alice), own_id(&bob));
    until(
        &mut server,
        &mut alice,
        "bob arriving",
        |heard| matches!(heard, Heard::Player { name, .. } if name == "bob"),
    );
    alice.use_entity(alice_id, bob_id, true);
    // A bare fist takes half a heart, and everyone sees him flinch.
    until(&mut server, &mut bob, "the blow", |heard| {
        *heard == Heard::Health(19)
    });
    until(&mut server, &mut alice, "bob flinching", |heard| {
        *heard
            == Heard::Status {
                entity: bob_id,
                status: 2,
            }
    });
}

#[test]
fn primed_tnt_falls_and_a_riders_motion_steers() {
    let mut server = server("bodies");
    let (mut player, [x, feet, z]) = join(&mut server, "rider");
    let (_, body) = server.host_mut().find("rider").unwrap();

    // Nothing integrates a player here, but TNT still drops.
    let world = overworld(&mut server);
    let start = Vec3::new(x as f32 + 3.0, feet as f32 + 12.0, z as f32);
    let tnt = prime_tnt(&mut world.commands(), start, 200);
    world.flush();
    until(&mut server, &mut player, "the TNT", |heard| {
        matches!(heard, Heard::Object { kind: 50, .. })
    });
    // Bodies fall on frame time, so give it some.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        server.tick();
        std::thread::sleep(Duration::from_millis(10));
        let fallen = overworld(&mut server)
            .get::<Transform>(tnt)
            .unwrap()
            .translation;
        if fallen.y < start.y - 1.0 {
            break;
        }
        assert!(Instant::now() < deadline, "the TNT hangs at {fallen}");
    }

    // A rider's packet carries its own motion where a position would be:
    // straight ahead at yaw 0 is +Z, the forward key held down.
    let mut out = vec![13];
    for value in [0.0f64, -999.0, -999.0, 0.0178] {
        out.extend(value.to_be_bytes());
    }
    out.extend(0f32.to_be_bytes());
    out.extend(0f32.to_be_bytes());
    out.push(0);
    player.send(&out);
    let mut input =
        |server: &mut BetaServer| *overworld(server).get::<PlayerMovementInput>(body).unwrap();
    // The packet may take a tick or two to cross the socket.
    let mut held = input(&mut server);
    for _ in 0..200 {
        if held.forward > 0.9 {
            break;
        }
        server.tick();
        std::thread::sleep(Duration::from_millis(2));
        held = input(&mut server);
    }
    assert!(held.forward > 0.9 && held.strafe == 0.0, "{held:?}");
    // On foot again, the keys are the client's own business.
    player.stand(x, feet, z);
    for _ in 0..200 {
        if input(&mut server).forward == 0.0 {
            break;
        }
        server.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    let released = input(&mut server);
    assert_eq!((released.forward, released.strafe), (0.0, 0.0));
}
