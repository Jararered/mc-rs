//! A Beta 1.7.3 client's side of the wire, played against `BetaServer`.

use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use flate2::read::ZlibDecoder;
use game::networking::beta::BetaServer;
use game::networking::beta::ServerConfig;
use game::networking::beta::codec::ClientPacket;
use game::networking::beta::codec::ReadError;
use game::networking::beta::codec::read_packet;
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
    Health(i16),
    Respawn,
    /// `x`, the eyes, the feet, `z`.
    Position([f64; 4]),
    /// `Packet20NamedEntitySpawn`: another player, and where their feet are
    /// in thirty-seconds of a block.
    Player {
        entity: i32,
        name: String,
        at: [i32; 3],
    },
    Moved {
        entity: i32,
        at: [i32; 3],
    },
    Gone(i32),
    Swing(i32),
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
    BlockChange {
        at: [i32; 3],
        block: u8,
    },
    WindowItems(Vec<Option<(i16, i8, i16)>>),
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
}

/// One server packet from the front of `bytes`, or `None` until it is whole.
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
        18 => {
            let entity = w.i32()?;
            w.u8()?;
            Heard::Swing(entity)
        }
        20 => {
            let entity = w.i32()?;
            let name = w.string()?;
            let at = [w.i32()?, w.i32()?, w.i32()?];
            w.take(4)?;
            Heard::Player { entity, name, at }
        }
        29 => Heard::Gone(w.i32()?),
        34 => {
            let entity = w.i32()?;
            let at = [w.i32()?, w.i32()?, w.i32()?];
            w.take(2)?;
            Heard::Moved { entity, at }
        }
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
        104 => {
            w.u8()?;
            let count = w.i16()?;
            let mut slots = Vec::new();
            for _ in 0..count {
                let id = w.i16()?;
                slots.push(if id < 0 {
                    None
                } else {
                    Some((id, w.u8()? as i8, w.i16()?))
                });
            }
            Heard::WindowItems(slots)
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
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut from = 0;
    loop {
        server.tick();
        player.read();
        if let Some(found) = player.heard[from..].iter().find(|heard| wanted(heard)) {
            return found.clone();
        }
        from = player.heard.len();
        assert!(Instant::now() < deadline, "never heard {what}");
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
        |heard| matches!(heard, Heard::WindowItems(slots) if slots.len() == 45),
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

    bob.stand(x + 1.5, feet, z);
    bob.send(&[&[18u8][..], &entity.to_be_bytes(), &[1]].concat());
    until(&mut server, &mut alice, "bob moving", |heard| {
        *heard
            == Heard::Moved {
                entity,
                at: [fixed(x + 1.5), fixed(feet), fixed(z)],
            }
    });
    assert!(alice.heard.contains(&Heard::Swing(entity)));

    drop(bob);
    until(&mut server, &mut alice, "bob leaving", |heard| {
        *heard == Heard::Gone(entity)
    });
}
