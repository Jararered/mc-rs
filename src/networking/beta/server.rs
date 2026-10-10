//! `NetLoginHandler` and `NetServerHandler`: Beta clients joined to a
//! [`WorldHost`].
//!
//! One thread does everything. Sockets are non-blocking; each [`tick`] reads
//! what has arrived, applies it to the players, runs the world for a tick and
//! queues what each client should hear about. A player's hands become the
//! same [`PlayerAction`]s a local player's mouse does.
//!
//! [`tick`]: BetaServer::tick

use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::io::Read;
use std::io::Write;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::net::TcpStream;
use std::net::ToSocketAddrs;

use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use flate2::Compression;
use flate2::write::ZlibEncoder;

use super::PROTOCOL_VERSION;
use super::codec::ClientPacket;
use super::codec::ReadError;
use super::codec::WireStack;
use super::codec::Writer;
use super::codec::read_packet;
use crate::app::settings::GameSettings;
use crate::chat::ChatHistory;
use crate::chat::ChatPlugin;
use crate::chat::ChatSubmission;
use crate::entity::EntitySize;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::item::tools::mine_step;
use crate::physics::BlockFace;
use crate::physics::BlockHit;
use crate::player::PlayerHealth;
use crate::player::SurvivalPlugin;
use crate::player::actions::Action;
use crate::player::actions::PlayerAction;
use crate::player::actions::PlayerActionsPlugin;
use crate::player::sleep::SleepPlugin;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::dimension::Dimension;
use crate::world::host::WorldHost;
use crate::world::lighting::LightCache;
use crate::world::persistence::WorldStorage;
use crate::world::persistence::beta_chunk_bytes;
use crate::world::plugin::WorldPlugin;
use crate::world::streaming::Viewers;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::TICK_SECONDS;

/// Chunks sent to one client in a tick, so a join does not stall the rest.
const CHUNKS_PER_TICK: usize = 6;
/// A client this far behind on reading is dropped.
const MAX_BACKLOG: usize = 32 * 1024 * 1024;
/// `Packet6SpawnPosition`: the world spawn.
const SPAWN: [i32; 3] = [8, 64, 8];

/// How a [`BetaServer`] runs.
#[derive(Clone, Copy, Debug)]
pub struct ServerConfig {
    /// Chunks each way a client is sent, and the world keeps loaded around
    /// each player.
    pub view_distance: i32,
    pub autosave_seconds: f32,
    /// Run the world on Peaceful whatever it was saved with. Clients are not
    /// told about mobs yet, so a monster would be one they cannot see.
    pub peaceful: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            view_distance: 8,
            autosave_seconds: 60.0,
            peaceful: true,
        }
    }
}

/// How far a connection has got.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Nothing but a handshake and a login is read.
    Login,
    /// In the world, waiting for the ground under them. `fresh` players have
    /// no saved position and are stood on the surface.
    Joining {
        fresh: bool,
    },
    Playing,
}

struct Client {
    stream: TcpStream,
    address: SocketAddr,
    inbox: Vec<u8>,
    out: Writer,
    closed: bool,
    stage: Stage,
    name: String,
    dimension: Dimension,
    /// Chunks the client holds.
    sent: HashSet<ChunkPosition>,
    /// Where the client and the server last agreed the player's eyes are.
    known: Vec3,
    /// The client was moved and has not confirmed it (`hasMoved`): until it
    /// echoes this position, what it says about where it is is stale.
    awaiting: Option<Vec3>,
    reposition: bool,
    /// The inventory window as last sent, or empty to send it again.
    inventory: Vec<Option<WireStack>>,
    health: Option<u8>,
    /// Blocks to tell the client about whether or not they changed, after a
    /// click the client may have predicted wrongly.
    resend: Vec<IVec3>,
    /// The client swung its arm since the others were last told.
    swung: bool,
    /// The other players this client has been shown, by entity id, with
    /// where it last heard each of them was.
    shown: HashMap<i32, ([f64; 3], f32, f32)>,
}

/// A player in the world, as the other clients are told about them.
struct Seen {
    name: String,
    dimension: Dimension,
    entity: i32,
    feet: [f64; 3],
    yaw: f32,
    pitch: f32,
    held: i16,
    swung: bool,
}

/// A world Beta 1.7.3 clients can join.
pub struct BetaServer {
    listener: TcpListener,
    host: WorldHost,
    clients: Vec<Client>,
    config: ServerConfig,
    ticks: u64,
    /// How much of each dimension's chat has been relayed.
    chat_seen: HashMap<Dimension, u64>,
}

/// The simulation a hosted dimension runs. There is no window, renderer or
/// input; `WorldRenderingPlugin` is here for chunk streaming, which still
/// lights chunks through its mesh jobs.
fn build_world(app: &mut App, view_distance: i32) {
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins((
            WorldPlugin,
            crate::rendering::WorldRenderingPlugin,
            PlayerActionsPlugin,
            SurvivalPlugin,
            SleepPlugin,
            ChatPlugin,
        ));
    app.world_mut()
        .resource_mut::<GameSettings>()
        .render_distance = view_distance;
}

impl BetaServer {
    /// Listen on `address` and host `storage`.
    pub fn bind(
        address: impl ToSocketAddrs,
        storage: WorldStorage,
        config: ServerConfig,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let view_distance = config.view_distance;
        let mut host = WorldHost::new(storage, config.autosave_seconds, move |app| {
            build_world(app, view_distance);
        });
        if config.peaceful {
            host.set_difficulty(crate::world::difficulty::Difficulty::Peaceful);
        }
        Ok(Self {
            listener,
            host,
            clients: Vec::new(),
            config,
            ticks: 0,
            chat_seen: HashMap::new(),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn host(&self) -> &WorldHost {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut WorldHost {
        &mut self.host
    }

    /// The names of the players who are in the world.
    pub fn players(&self) -> Vec<&str> {
        self.clients
            .iter()
            .filter(|client| client.stage != Stage::Login && !client.closed)
            .map(|client| client.name.as_str())
            .collect()
    }

    /// One twentieth of a second: read the clients, run the world, and tell
    /// the clients what happened.
    pub fn tick(&mut self) {
        self.accept();
        for client in &mut self.clients {
            receive(client, &mut self.host);
        }
        self.host.update(TICK_SECONDS);
        self.ticks += 1;

        let mut changes: HashMap<Dimension, Vec<IVec3>> = HashMap::new();
        let mut chats: HashMap<Dimension, Vec<String>> = HashMap::new();
        for dimension in self.host.loaded() {
            let Some(world) = self.host.world_mut(dimension) else {
                continue;
            };
            if let Some(mut streaming) = world.get_resource_mut::<WorldStreaming>() {
                streaming.log_block_updates();
                let mut changed = streaming.take_block_updates();
                changed.sort_by_key(|cell| (cell.x, cell.z, cell.y));
                changed.dedup();
                changes.insert(dimension, changed);
            }
            if let Some(history) = world.get_resource::<ChatHistory>() {
                let seen = self.chat_seen.entry(dimension).or_default();
                // A dimension that unloaded and came back starts over.
                if history.pushed() < *seen {
                    *seen = 0;
                }
                let new = (history.pushed() - *seen) as usize;
                *seen = history.pushed();
                let mut lines: Vec<String> = history
                    .messages()
                    .take(new)
                    .map(|message| message.text.clone())
                    .collect();
                lines.reverse();
                chats.insert(dimension, lines);
            }
        }

        // `EntityTracker`, for players only so far.
        let mut roster = Vec::new();
        for client in &mut self.clients {
            let swung = std::mem::take(&mut client.swung);
            if client.stage != Stage::Playing || client.closed {
                continue;
            }
            let Some((dimension, entity)) = self.host.find(&client.name) else {
                continue;
            };
            let Some(player) = self
                .host
                .world(dimension)
                .and_then(|world| world.get_entity(entity).ok())
            else {
                continue;
            };
            let Some(transform) = player.get::<Transform>() else {
                continue;
            };
            let (yaw, pitch) = beta_angles(transform.rotation);
            let eyes = transform.translation;
            roster.push(Seen {
                name: client.name.clone(),
                dimension,
                entity: entity.index_u32() as i32,
                feet: [
                    f64::from(eyes.x),
                    f64::from(eyes.y - EntitySize::PLAYER.y_offset),
                    f64::from(eyes.z),
                ],
                yaw,
                pitch,
                held: player
                    .get::<Hotbar>()
                    .and_then(Hotbar::selected_stack)
                    .map_or(0, |stack| stack.item().as_u16() as i16),
                swung,
            });
        }

        let time = self.host.world_time();
        for client in &mut self.clients {
            if client.stage != Stage::Login && !client.closed {
                send_state(
                    client,
                    &mut self.host,
                    &self.config,
                    self.ticks,
                    time,
                    &changes,
                    &chats,
                    &roster,
                );
            }
            flush(client);
        }
        for client in self.clients.extract_if(.., |client| client.closed) {
            if client.stage != Stage::Login {
                info!("{} left the game", client.name);
                if let Err(error) = self.host.leave(&client.name) {
                    warn!("Could not save {}: {error}", client.name);
                }
            }
        }
    }

    fn accept(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((stream, address)) => {
                    if stream.set_nonblocking(true).is_err() {
                        continue;
                    }
                    // Movement packets are tiny and frequent.
                    let _ = stream.set_nodelay(true);
                    self.clients.push(Client {
                        stream,
                        address,
                        inbox: Vec::new(),
                        out: Writer::default(),
                        closed: false,
                        stage: Stage::Login,
                        name: String::new(),
                        dimension: Dimension::Overworld,
                        sent: HashSet::new(),
                        known: Vec3::ZERO,
                        awaiting: None,
                        reposition: false,
                        inventory: Vec::new(),
                        health: None,
                        resend: Vec::new(),
                        swung: false,
                        shown: HashMap::new(),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return,
                Err(error) => {
                    warn!("Could not accept a connection: {error}");
                    return;
                }
            }
        }
    }

    /// Send everyone away, then save the players and the manifest. Each
    /// dimension's chunks are saved as it unloads, so keep calling
    /// [`Self::tick`] until [`WorldHost::loaded`] is empty to save those.
    pub fn close(&mut self) {
        for client in &mut self.clients {
            client.out.kick("Server closed");
            flush(client);
            if client.stage != Stage::Login
                && let Err(error) = self.host.leave(&client.name)
            {
                warn!("Could not save {}: {error}", client.name);
            }
        }
        self.clients.clear();
        if let Err(error) = self.host.save_level() {
            warn!("Could not save the world: {error}");
        }
    }
}

/// Read what `client` has sent and apply it.
fn receive(client: &mut Client, host: &mut WorldHost) {
    let mut buffer = [0u8; 4096];
    loop {
        match client.stream.read(&mut buffer) {
            Ok(0) => {
                client.closed = true;
                break;
            }
            Ok(count) => client.inbox.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => {
                client.closed = true;
                break;
            }
        }
    }
    let mut at = 0;
    while !client.closed {
        match read_packet(&client.inbox[at..]) {
            Ok((packet, length)) => {
                at += length;
                handle(client, host, packet);
            }
            Err(ReadError::Incomplete) => break,
            Err(ReadError::Invalid(what)) => {
                warn!("Dropping {}: {what}", client.address);
                client.out.kick("Bad packet");
                client.closed = true;
            }
        }
    }
    client.inbox.drain(..at);
}

/// Write as much of what is queued for `client` as its socket takes.
fn flush(client: &mut Client) {
    let mut written = 0;
    while written < client.out.0.len() {
        match client.stream.write(&client.out.0[written..]) {
            Ok(0) => {
                client.closed = true;
                break;
            }
            Ok(count) => written += count,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => {
                client.closed = true;
                break;
            }
        }
    }
    client.out.0.drain(..written);
    if client.out.0.len() > MAX_BACKLOG {
        client.closed = true;
    }
}

fn kick(client: &mut Client, reason: &str) {
    client.out.kick(reason);
    client.closed = true;
}

fn dimension_byte(dimension: Dimension) -> i8 {
    match dimension {
        Dimension::Overworld => 0,
        Dimension::Nether => -1,
    }
}

/// `Packet14BlockDig.face` and `Packet15Place.direction`.
fn face(direction: u8) -> Option<BlockFace> {
    Some(match direction {
        0 => BlockFace::Down,
        1 => BlockFace::Up,
        2 => BlockFace::North,
        3 => BlockFace::South,
        4 => BlockFace::West,
        5 => BlockFace::East,
        _ => return None,
    })
}

/// Beta's yaw and pitch, in degrees, as a Bevy rotation. Beta's yaw 0 looks
/// along +Z and its pitch grows downward.
fn rotation(yaw: f32, pitch: f32) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        std::f32::consts::PI - yaw.to_radians(),
        -pitch.to_radians(),
        0.0,
    )
}

fn beta_angles(rotation: Quat) -> (f32, f32) {
    let (yaw, pitch, _) = rotation.to_euler(EulerRot::YXZ);
    (
        (std::f32::consts::PI - yaw).to_degrees(),
        -pitch.to_degrees(),
    )
}

fn handle(client: &mut Client, host: &mut WorldHost, packet: ClientPacket) {
    match (client.stage, packet) {
        (_, ClientPacket::KeepAlive) => {}
        (_, ClientPacket::Disconnect(_)) => client.closed = true,
        // No name is checked with minecraft.net: this is an offline server.
        (Stage::Login, ClientPacket::Handshake { .. }) => client.out.handshake("-"),
        (Stage::Login, ClientPacket::Login { protocol, username }) => {
            if protocol != PROTOCOL_VERSION {
                let reason = if protocol > PROTOCOL_VERSION {
                    "Outdated server!"
                } else {
                    "Outdated client!"
                };
                return kick(client, reason);
            }
            let valid = !username.is_empty()
                && username.len() <= 16
                && username
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !valid {
                return kick(client, "Invalid username");
            }
            if host.find(&username).is_some() {
                return kick(client, "You are already playing on this server");
            }
            let fresh = host.storage().load_named_player(&username).is_none();
            let (dimension, entity) = host.join(&username);
            info!("{username} joined the game from {}", client.address);
            client.name = username;
            client.dimension = dimension;
            client.stage = Stage::Joining { fresh };
            client.out.login(
                entity.index_u32() as i32,
                host.storage().seed() as i64,
                dimension_byte(dimension),
            );
            client.out.spawn_position(SPAWN[0], SPAWN[1], SPAWN[2]);
            client.out.time(host.world_time() as i64);
        }
        (Stage::Login, _) => kick(client, "Log in first"),
        (Stage::Joining { .. }, _) => {}
        (Stage::Playing, packet) => play(client, host, packet),
    }
}

/// `NetServerHandler`: a packet from a player who is in the world.
fn play(client: &mut Client, host: &mut WorldHost, packet: ClientPacket) {
    let Some((dimension, entity)) = host.find(&client.name) else {
        return;
    };
    let Some(world) = host.world_mut(dimension) else {
        return;
    };
    let act = |world: &mut World, action: Action| {
        if let Some(mut actions) = world.get_resource_mut::<Messages<PlayerAction>>() {
            actions.write(PlayerAction {
                player: entity,
                action,
            });
        }
    };
    match packet {
        ClientPacket::Move { position, look, .. } => {
            let Some(mut transform) = world.get_mut::<Transform>(entity) else {
                return;
            };
            if let Some([yaw, pitch]) = look {
                transform.rotation = rotation(yaw, pitch);
            }
            // A rider sends -999 for both heights and no position of its own.
            if let Some([x, feet, _, z]) = position
                && feet > -900.0
                && [x, feet, z].iter().all(|value| value.is_finite())
            {
                let eyes = Vec3::new(
                    x as f32,
                    feet as f32 + EntitySize::PLAYER.y_offset,
                    z as f32,
                );
                match client.awaiting {
                    Some(target) if eyes.distance(target) > 0.1 => {}
                    _ => {
                        client.awaiting = None;
                        transform.translation = eyes;
                        client.known = eyes;
                    }
                }
            }
        }
        ClientPacket::Dig {
            status,
            x,
            y,
            z,
            face: direction,
        } => {
            // Status 4 drops the held item and names no block.
            if status == 4 {
                client.inventory.clear();
                return act(world, Action::DropItem);
            }
            let Some(block) = world.resource::<WorldChunks>().block_at(x, y, z) else {
                return;
            };
            let hit = BlockHit {
                x,
                y,
                z,
                face: face(direction).unwrap_or(BlockFace::Up),
                block,
            };
            client.resend.push(IVec3::new(x, y, z));
            match status {
                0 => {
                    act(world, Action::StartDig { hit });
                    // `ItemInWorldManager.blockClicked`: a block that breaks
                    // in one blow goes on the click.
                    let tool = world.get::<Hotbar>(entity).and_then(Hotbar::selected_stack);
                    if mine_step(block, tool, true, false) >= 1.0 {
                        act(world, Action::Break { hit });
                    }
                }
                // The client timed the dig. Beta checks it took long enough;
                // that is not done yet.
                2 => act(world, Action::Break { hit }),
                _ => {}
            }
        }
        ClientPacket::Place {
            x, y, z, direction, ..
        } => {
            let Some(transform) = world.get::<Transform>(entity).copied() else {
                return;
            };
            // Direction 255 uses the held item on nothing.
            let hit = face(direction).and_then(|face| {
                let block = world.resource::<WorldChunks>().block_at(x, y, z)?;
                Some(BlockHit {
                    x,
                    y,
                    z,
                    face,
                    block,
                })
            });
            if let Some(hit) = hit {
                let (nx, ny, nz) = hit.face.neighbor(hit.x, hit.y, hit.z);
                client
                    .resend
                    .extend([IVec3::new(x, y, z), IVec3::new(nx, ny, nz)]);
            }
            // The client has already taken the item out of its own hand.
            client.inventory.clear();
            act(
                world,
                Action::Use {
                    hit,
                    origin: transform.translation,
                    look: transform.rotation * Vec3::NEG_Z,
                    click: true,
                },
            );
        }
        ClientPacket::Animation => client.swung = true,
        ClientPacket::HeldSlot(slot) => {
            if let Some(mut hotbar) = world.get_mut::<Hotbar>(entity)
                && let Ok(slot) = usize::try_from(slot)
                && slot < hotbar.slots.len()
            {
                hotbar.selected = slot;
            }
        }
        ClientPacket::Chat(text) => {
            if let Some(mut submissions) = world.get_resource_mut::<Messages<ChatSubmission>>() {
                submissions.write(ChatSubmission::from_player(entity, text));
            }
        }
        // What the client did in a window is not applied yet, so put its
        // inventory back the way the server has it.
        ClientPacket::WindowClick | ClientPacket::CloseWindow => client.inventory.clear(),
        ClientPacket::Respawn => {
            client.out.respawn(dimension_byte(dimension));
            client.reposition = true;
            client.health = None;
            client.inventory.clear();
        }
        _ => {}
    }
}

fn wire(stack: Option<ItemStack>) -> Option<WireStack> {
    stack.map(|stack| {
        (
            stack.item().as_u16() as i16,
            stack.count() as i8,
            stack.data() as i16,
        )
    })
}

/// `ContainerPlayer`'s slots: the crafting result and grid, the armor from
/// the helmet down, the main inventory, then the hotbar.
fn window_slots(hotbar: &Hotbar, inventory: &Inventory) -> Vec<Option<WireStack>> {
    let mut slots = vec![None];
    slots.extend(inventory.crafting.iter().map(|slot| wire(*slot)));
    slots.extend(inventory.armor.iter().map(|slot| wire(*slot)));
    slots.extend(inventory.main.iter().map(|slot| wire(*slot)));
    slots.extend(hotbar.slots.iter().map(|slot| wire(*slot)));
    slots
}

/// Whether `position` is ready for a client: populated among its neighbors
/// and lit.
fn ready(world: &World, position: ChunkPosition) -> bool {
    let chunks = world.resource::<WorldChunks>();
    chunks.contains(position)
        && world
            .get_resource::<LightCache>()
            .is_some_and(|light| light.contains(position))
        && world
            .get_resource::<WorldStreaming>()
            .is_some_and(|streaming| streaming.neighborhood_finished(chunks, position))
}

/// Queue what `client` should hear about this tick.
#[allow(clippy::too_many_lines)]
fn send_state(
    client: &mut Client,
    host: &mut WorldHost,
    config: &ServerConfig,
    ticks: u64,
    time: u64,
    changes: &HashMap<Dimension, Vec<IVec3>>,
    chats: &HashMap<Dimension, Vec<String>>,
    roster: &[Seen],
) {
    let Some((dimension, entity)) = host.find(&client.name) else {
        return;
    };
    if dimension != client.dimension {
        // `sendPlayerToOtherDimension`: the client drops its world.
        client.dimension = dimension;
        client.out.respawn(dimension_byte(dimension));
        client.sent.clear();
        client.shown.clear();
        client.reposition = true;
    }
    let Some(world) = host.world_mut(dimension) else {
        return;
    };
    let Some(mut transform) = world.get::<Transform>(entity).copied() else {
        return;
    };
    let center = ChunkPosition::from_world(transform.translation.x, transform.translation.z);

    // Chunks, nearest first, and the ones left behind.
    let viewers = Viewers::new([center]);
    let mut wanted = viewers.positions(config.view_distance);
    viewers.sort_by_distance(&mut wanted);
    let mut budget = CHUNKS_PER_TICK;
    for position in wanted {
        if budget == 0 {
            break;
        }
        if client.sent.contains(&position) || !ready(world, position) {
            continue;
        }
        let Some(generated) = world.resource::<WorldChunks>().get(position) else {
            continue;
        };
        let light = world.resource::<LightCache>().cells(position);
        let bytes = beta_chunk_bytes(&generated.chunk, light.as_deref(), dimension.has_sky());
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        let Ok(deflated) = encoder.write_all(&bytes).and_then(|()| encoder.finish()) else {
            continue;
        };
        client.out.pre_chunk(position.x, position.z, true);
        client.out.map_chunk(position.x, position.z, &deflated);
        client.sent.insert(position);
        budget -= 1;
    }
    let keep = config.view_distance + 2;
    let out = &mut client.out;
    client.sent.retain(|position| {
        let near = viewers.within(*position, keep);
        if !near {
            out.pre_chunk(position.x, position.z, false);
        }
        near
    });

    if let Stage::Joining { fresh } = client.stage {
        if !client.sent.contains(&center) {
            return;
        }
        if fresh {
            // Stand a new player on the ground rather than at a fixed height.
            let surface = world
                .resource::<WorldChunks>()
                .get(center)
                .map_or(64, |chunk| {
                    chunk.heightmap.get(
                        (transform.translation.x.floor() as i32).rem_euclid(16) as usize,
                        (transform.translation.z.floor() as i32).rem_euclid(16) as usize,
                    )
                });
            transform.translation.y = f32::from(surface) + EntitySize::PLAYER.y_offset;
            if let Some(mut live) = world.get_mut::<Transform>(entity) {
                live.translation = transform.translation;
            }
        }
        client.stage = Stage::Playing;
        client.reposition = true;
    }

    // The server moved the player: a teleport, a respawn, a bed.
    if client.reposition
        || (client.awaiting.is_none() && transform.translation.distance(client.known) > 0.05)
    {
        client.reposition = false;
        let eyes = transform.translation;
        let (yaw, pitch) = beta_angles(transform.rotation);
        client.out.position(
            f64::from(eyes.x),
            f64::from(eyes.y - EntitySize::PLAYER.y_offset),
            f64::from(eyes.z),
            yaw,
            pitch,
        );
        client.known = eyes;
        client.awaiting = Some(eyes);
    }

    let mut cells = std::mem::take(&mut client.resend);
    if let Some(changed) = changes.get(&dimension) {
        cells.extend(changed);
    }
    let chunks = world.resource::<WorldChunks>();
    for cell in cells {
        if !(0..128).contains(&cell.y)
            || !client
                .sent
                .contains(&ChunkPosition::from_block(cell.x, cell.z))
        {
            continue;
        }
        if let Some(block) = chunks.block_at(cell.x, cell.y, cell.z) {
            client.out.block_change(
                cell.x,
                cell.y,
                cell.z,
                block.as_u8(),
                chunks.metadata_at(cell.x, cell.y, cell.z),
            );
        }
    }

    if ticks % 20 == 0 {
        client.out.keep_alive();
        client.out.time(time as i64);
    }
    if let (Some(hotbar), Some(inventory)) =
        (world.get::<Hotbar>(entity), world.get::<Inventory>(entity))
    {
        let slots = window_slots(hotbar, inventory);
        if slots != client.inventory {
            client.out.window_items(0, &slots);
            client.inventory = slots;
        }
    }
    if let Some(health) = world.get::<PlayerHealth>(entity)
        && client.health != Some(health.current)
    {
        client.health = Some(health.current);
        client.out.health(i16::from(health.current));
    }
    for line in chats.get(&dimension).into_iter().flatten() {
        client.out.chat(line);
    }

    // The other players here. A new arrival's held item is as of when they
    // came into view; changes to it are not relayed yet.
    if client.stage != Stage::Playing {
        return;
    }
    let others = || {
        roster
            .iter()
            .filter(|other| other.dimension == dimension && other.name != client.name)
    };
    let out = &mut client.out;
    client.shown.retain(|entity, _| {
        let here = others().any(|other| other.entity == *entity);
        if !here {
            out.destroy_entity(*entity);
        }
        here
    });
    for other in others() {
        let now = (other.feet, other.yaw, other.pitch);
        match client.shown.insert(other.entity, now) {
            None => client.out.named_entity_spawn(
                other.entity,
                &other.name,
                other.feet,
                other.yaw,
                other.pitch,
                other.held,
            ),
            Some(before) if before != now => {
                client
                    .out
                    .entity_teleport(other.entity, other.feet, other.yaw, other.pitch);
            }
            Some(_) => {}
        }
        if other.swung {
            client.out.swing(other.entity);
        }
    }
}
