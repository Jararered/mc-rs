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
use super::codec::Writer;
use super::codec::read_packet;
use super::tracker::Tracker;
use super::tracker::Viewer;
use super::tracker::beta_angles;
use super::tracker::survey;
use super::windows::Windows;
use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::chat::ChatHistory;
use crate::chat::ChatPlugin;
use crate::chat::ChatSubmission;
use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::boat::Boat;
use crate::entity::drops::items::spawn_thrown_item;
use crate::entity::explosion::Exploded;
use crate::entity::minecart::Minecart;
use crate::entity::mobs::Mob;
use crate::entity::projectiles::Fireball;
use crate::inventory::Hotbar;
use crate::item::ItemStack;
use crate::item::tools::mine_step;
use crate::physics::BlockFace;
use crate::physics::BlockHit;
use crate::player::ClientRespawn;
use crate::player::PlayerHealth;
use crate::player::PlayerMovementInput;
use crate::player::SurvivalPlugin;
use crate::player::actions::Action;
use crate::player::actions::PlayerAction;
use crate::player::actions::PlayerActionsPlugin;
use crate::player::actions::Pointed;
use crate::player::actions::WindowOpen;
use crate::player::sleep::PlayerSleep;
use crate::player::sleep::SleepPlugin;
use crate::random::ItemRng;
use crate::world::block_ticks::AuxEffect;
use crate::world::block_ticks::NotePlayed;
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
use crate::world::weather::LightningStrike;
use crate::world::weather::WorldWeather;

/// Chunks sent to one client in a tick, so a join does not stall the rest.
const CHUNKS_PER_TICK: usize = 6;
/// A client this far behind on reading is dropped.
const MAX_BACKLOG: usize = 32 * 1024 * 1024;
/// `Packet6SpawnPosition`: the world spawn.
const SPAWN: [i32; 3] = [8, 64, 8];
/// How far away an explosion, a note or a door is heard
/// (`sendPacketToPlayersAroundPoint`).
const EFFECT_RANGE: f32 = 64.0;
/// How far away a lightning bolt is seen.
const LIGHTNING_RANGE: f32 = 512.0;
/// `Packet7UseEntity` reaches this far.
const ENTITY_REACH: f32 = 6.0;

/// How a [`BetaServer`] runs.
#[derive(Clone, Copy, Debug)]
pub struct ServerConfig {
    /// Chunks each way a client is sent, and the world keeps loaded around
    /// each player.
    pub view_distance: i32,
    pub autosave_seconds: f32,
    /// Run the world on Peaceful whatever it was saved with.
    pub peaceful: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            view_distance: 8,
            autosave_seconds: 60.0,
            peaceful: false,
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
    /// The entity id the client knows its own player by. It stays the same
    /// through a change of dimension, where the player's entity does not.
    id: i32,
    dimension: Dimension,
    /// Where the player is as of this tick's world update.
    place: Option<(Dimension, Entity)>,
    /// Chunks the client holds.
    sent: HashSet<ChunkPosition>,
    /// Where the client and the server last agreed the player's eyes are.
    known: Vec3,
    /// The client was moved and has not confirmed it (`hasMoved`): until it
    /// echoes this position, what it says about where it is is stale.
    awaiting: Option<Vec3>,
    reposition: bool,
    /// The inventory and whatever container is open, as the client has them.
    windows: Windows,
    health: Option<u8>,
    /// The client is on its death screen.
    dead: bool,
    /// The client has been told it is raining.
    raining: bool,
    /// Blocks to tell the client about whether or not they changed, after a
    /// click the client may have predicted wrongly.
    resend: Vec<IVec3>,
    /// The client swung its arm since the others were last told.
    swung: bool,
    /// The entities this client has been shown, by id.
    shown: HashSet<i32>,
    /// The riders it has been told about, and what each rides.
    attached: HashMap<i32, i32>,
}

/// `World.playAuxSFX` from a player's click: a sound the others near it hear.
struct Effect {
    dimension: Dimension,
    /// The client that made it, which played it for itself.
    except: i32,
    at: IVec3,
    effect: i32,
}

/// A sign was written on, and everyone holding its chunk is told.
struct SignChange {
    dimension: Dimension,
    at: IVec3,
    lines: [String; 4],
}

/// What `receive` needs besides the client and the world.
struct Shared<'a> {
    tracker: &'a mut Tracker,
    effects: &'a mut Vec<Effect>,
    signs: &'a mut Vec<SignChange>,
    rng: &'a mut ItemRng,
}

/// What a hosted dimension has to say that is not in its components: the
/// messages a client has to hear about, kept from one server tick to the
/// next.
#[derive(Resource, Default)]
struct Outbox {
    windows: Vec<WindowOpen>,
    blasts: Vec<Exploded>,
    strikes: Vec<Vec3>,
    notes: Vec<NotePlayed>,
    aux: Vec<AuxEffect>,
}

fn fill_outbox(
    mut outbox: ResMut<Outbox>,
    mut windows: MessageReader<WindowOpen>,
    mut blasts: MessageReader<Exploded>,
    mut strikes: MessageReader<LightningStrike>,
    mut notes: MessageReader<NotePlayed>,
    mut aux: MessageReader<AuxEffect>,
) {
    outbox.aux.extend(aux.read().copied());
    outbox.windows.extend(windows.read().copied());
    outbox.blasts.extend(blasts.read().cloned());
    outbox.strikes.extend(strikes.read().map(|strike| strike.0));
    outbox.notes.extend(notes.read().copied());
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
    tracker: Tracker,
    effects: Vec<Effect>,
    signs: Vec<SignChange>,
    rng: ItemRng,
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
            // Dropped items move, age and are picked up here.
            crate::entity::drops::items::DroppedItemPlugin,
            // Primed TNT falls. Players are left to their clients.
            crate::physics::BodyPhysicsPlugin,
            PlayerActionsPlugin,
            SurvivalPlugin,
            SleepPlugin,
            ChatPlugin,
        ))
        .init_resource::<Outbox>()
        .add_message::<WindowOpen>()
        .add_message::<Exploded>()
        .add_message::<LightningStrike>()
        .add_message::<NotePlayed>()
        .add_message::<AuxEffect>()
        .add_systems(Last, fill_outbox);
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
            tracker: Tracker::default(),
            effects: Vec::new(),
            signs: Vec::new(),
            rng: ItemRng::default(),
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
    #[allow(clippy::too_many_lines)]
    pub fn tick(&mut self) {
        self.accept();
        let mut shared = Shared {
            tracker: &mut self.tracker,
            effects: &mut self.effects,
            signs: &mut self.signs,
            rng: &mut self.rng,
        };
        for client in &mut self.clients {
            receive(client, &mut self.host, &mut shared);
        }
        self.host.update(TICK_SECONDS);
        self.ticks += 1;
        for client in &mut self.clients {
            client.place = if client.stage == Stage::Login || client.closed {
                None
            } else {
                self.host.find(&client.name)
            };
        }

        let loaded = self.host.loaded();
        self.tracker.forget_all_but(&loaded);
        let mut changes: HashMap<Dimension, Vec<IVec3>> = HashMap::new();
        let mut chats: HashMap<Dimension, Vec<String>> = HashMap::new();
        let mut events: HashMap<Dimension, Outbox> = HashMap::new();
        for &dimension in &loaded {
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
            if let Some(mut outbox) = world.get_resource_mut::<Outbox>() {
                events.insert(dimension, std::mem::take(&mut *outbox));
            }
        }

        // `displayGUIChest` and its like: a click opened a container.
        for (&dimension, outbox) in &mut events {
            for opened in outbox.windows.drain(..) {
                let Some(client) = self
                    .clients
                    .iter_mut()
                    .find(|client| client.place == Some((dimension, opened.player)))
                else {
                    continue;
                };
                let Some(world) = self.host.world_mut(dimension) else {
                    continue;
                };
                let dropped =
                    client
                        .windows
                        .open(world, opened.player, opened.window, &mut client.out);
                throw(world, opened.player, &mut self.rng, dropped);
            }
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
                    &mut self.rng,
                );
            }
        }

        // `EntityTracker`: what each client can see of the others and of
        // everything else that moves.
        let ids: HashMap<String, i32> = self
            .clients
            .iter()
            .filter(|client| client.stage == Stage::Playing && !client.closed)
            .map(|client| (client.name.clone(), client.id))
            .collect();
        for &dimension in &loaded {
            let Some(world) = self.host.world_mut(dimension) else {
                continue;
            };
            let seen = survey(world);
            let mut viewers: Vec<Viewer> = self
                .clients
                .iter_mut()
                .filter(|client| client.stage == Stage::Playing && !client.closed)
                .filter_map(|client| {
                    let (place, entity) = client.place?;
                    (place == dimension && client.dimension == dimension).then_some(Viewer {
                        id: client.id,
                        entity,
                        at: client.known,
                        chunks: &client.sent,
                        shown: &mut client.shown,
                        attached: &mut client.attached,
                        out: &mut client.out,
                    })
                })
                .collect();
            self.tracker.update(dimension, &seen, &ids, &mut viewers);
            // Nothing integrates a remote player, so a hit's knockback is
            // spent once the clients have been told of it.
            for viewer in &viewers {
                if let Some(mut velocity) = world.get_mut::<Velocity>(viewer.entity)
                    && velocity.0 != Vec3::ZERO
                {
                    velocity.0 = Vec3::ZERO;
                }
            }
        }

        // What happened in the world that is not an entity or a block.
        let swings: Vec<(Dimension, i32)> = self
            .clients
            .iter_mut()
            .filter_map(|client| {
                (std::mem::take(&mut client.swung) && client.stage == Stage::Playing)
                    .then_some((client.dimension, client.id))
            })
            .collect();
        let effects = std::mem::take(&mut self.effects);
        let signs = std::mem::take(&mut self.signs);
        let mut bolts = Vec::new();
        for (dimension, outbox) in &events {
            for strike in &outbox.strikes {
                bolts.push((*dimension, self.tracker.allocate(), *strike));
            }
        }
        for client in &mut self.clients {
            if client.stage != Stage::Playing || client.closed {
                continue;
            }
            let known = client.known;
            let near = |at: Vec3, range: f32| at.distance_squared(known) < range * range;
            for (dimension, id) in &swings {
                if *dimension == client.dimension && client.shown.contains(id) {
                    client.out.animation(*id, 1);
                }
            }
            for effect in &effects {
                if effect.dimension == client.dimension
                    && effect.except != client.id
                    && near(effect.at.as_vec3(), EFFECT_RANGE)
                {
                    client
                        .out
                        .effect(effect.effect, effect.at.x, effect.at.y, effect.at.z, 0);
                }
            }
            for sign in &signs {
                if sign.dimension == client.dimension
                    && client
                        .sent
                        .contains(&ChunkPosition::from_block(sign.at.x, sign.at.z))
                {
                    client
                        .out
                        .update_sign(sign.at.x, sign.at.y, sign.at.z, &sign.lines);
                }
            }
            for (dimension, id, at) in &bolts {
                if *dimension == client.dimension && near(*at, LIGHTNING_RANGE) {
                    client
                        .out
                        .lightning(*id, [f64::from(at.x), f64::from(at.y), f64::from(at.z)]);
                }
            }
            let Some(outbox) = events.get(&client.dimension) else {
                continue;
            };
            for blast in &outbox.blasts {
                if near(blast.center, EFFECT_RANGE) {
                    let cells: Vec<[i32; 3]> =
                        blast.cells.iter().map(|cell| cell.to_array()).collect();
                    let center = blast.center;
                    client.out.explosion(
                        [
                            f64::from(center.x),
                            f64::from(center.y),
                            f64::from(center.z),
                        ],
                        blast.strength,
                        &cells,
                    );
                }
            }
            for aux in &outbox.aux {
                let at = aux.position;
                if near(at.as_vec3(), EFFECT_RANGE) {
                    client.out.effect(aux.effect, at.x, at.y, at.z, aux.data);
                }
            }
            for note in &outbox.notes {
                let at = note.position;
                if near(at.as_vec3(), EFFECT_RANGE) {
                    client
                        .out
                        .note(at.x, at.y, at.z, note.instrument, note.pitch);
                }
            }
        }

        for client in &mut self.clients {
            flush(client);
        }
        for mut client in self.clients.extract_if(.., |client| client.closed) {
            if client.stage != Stage::Login {
                info!("{} left the game", client.name);
                if let Some((dimension, entity)) = self.host.find(&client.name)
                    && let Some(world) = self.host.world_mut(dimension)
                {
                    // What they held on the cursor or left in a grid stays in
                    // the world rather than going with them.
                    let dropped = client.windows.close(world, entity);
                    throw(world, entity, &mut self.rng, dropped);
                }
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
                        id: 0,
                        dimension: Dimension::Overworld,
                        place: None,
                        sent: HashSet::new(),
                        known: Vec3::ZERO,
                        awaiting: None,
                        reposition: false,
                        windows: Windows::default(),
                        health: None,
                        dead: false,
                        raining: false,
                        resend: Vec::new(),
                        swung: false,
                        shown: HashSet::new(),
                        attached: HashMap::new(),
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

/// Throw `stacks` out in front of `player`, as `dropPlayerItem` does with
/// what a window can no longer hold.
fn throw(world: &mut World, player: Entity, rng: &mut ItemRng, stacks: Vec<ItemStack>) {
    if stacks.is_empty() {
        return;
    }
    let Some(eyes) = world.get::<Transform>(player).copied() else {
        return;
    };
    let look = eyes.rotation * Vec3::NEG_Z;
    let mut commands = world.commands();
    for stack in stacks {
        spawn_thrown_item(&mut commands, rng, &eyes, look, stack);
    }
    world.flush();
    if let Some(mut persistence) =
        world.get_resource_mut::<crate::world::persistence::WorldPersistence>()
    {
        persistence.mark_dirty(ChunkPosition::from_world(
            eyes.translation.x,
            eyes.translation.z,
        ));
    }
}

/// Read what `client` has sent and apply it.
fn receive(client: &mut Client, host: &mut WorldHost, shared: &mut Shared) {
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
                handle(client, host, shared, packet);
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

fn handle(client: &mut Client, host: &mut WorldHost, shared: &mut Shared, packet: ClientPacket) {
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
            // The death screen's button decides when this player gets up.
            if let Some(world) = host.world_mut(dimension) {
                world.entity_mut(entity).insert(ClientRespawn::default());
            }
            client.name = username;
            client.id = shared.tracker.allocate();
            client.dimension = dimension;
            client.stage = Stage::Joining { fresh };
            client.out.login(
                client.id,
                host.storage().seed() as i64,
                dimension_byte(dimension),
            );
            client.out.spawn_position(SPAWN[0], SPAWN[1], SPAWN[2]);
            client.out.time(host.world_time() as i64);
        }
        (Stage::Login, _) => kick(client, "Log in first"),
        (Stage::Joining { .. }, _) => {}
        (Stage::Playing, packet) => play(client, host, shared, packet),
    }
}

/// `NetServerHandler`: a packet from a player who is in the world.
#[allow(clippy::too_many_lines)]
fn play(client: &mut Client, host: &mut WorldHost, shared: &mut Shared, packet: ClientPacket) {
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
            // `handleFlying` for a rider: in the place of a position the
            // client sends its own `motionX/Z`, which is what a boat is
            // pushed by. A boat and a steered pig read the keys behind that
            // motion, so it is turned back into them.
            let steering = match position {
                Some([x, feet, _, z]) if feet <= -900.0 && x.is_finite() && z.is_finite() => {
                    // One airborne `moveFlying` and its drag: see
                    // `boat::rider_motion`.
                    const FULL: f32 = 0.02 * 0.98 * 0.91;
                    let (bevy_yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
                    let (sin, cos) = (std::f32::consts::PI - bevy_yaw).sin_cos();
                    let (x, z) = (x as f32, z as f32);
                    let key = |along: f32| {
                        let key = (along / FULL).clamp(-1.0, 1.0);
                        if key.abs() < 0.05 { 0.0 } else { key }
                    };
                    (key(x * cos + z * sin), key(z * cos - x * sin))
                }
                _ => (0.0, 0.0),
            };
            if let Some(mut input) = world.get_mut::<PlayerMovementInput>(entity)
                && (input.strafe, input.forward) != steering
            {
                (input.strafe, input.forward) = steering;
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
                client.windows.resend();
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
                // `BlockDoor.blockActivated` plays its creak for everyone
                // but the player, whose client has played it already.
                if matches!(hit.block, Block::WoodenDoor | Block::Trapdoor) {
                    shared.effects.push(Effect {
                        dimension,
                        except: client.id,
                        at: IVec3::new(x, y, z),
                        effect: 1003,
                    });
                }
            }
            // The client has already taken the item out of its own hand.
            client.windows.resend();
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
        ClientPacket::UseEntity { target, attack } => {
            let Some(target) = shared.tracker.lookup(dimension, target) else {
                return;
            };
            let Some(eyes) = world.get::<Transform>(entity).copied() else {
                return;
            };
            let Some(at) = world.get::<Transform>(target) else {
                return;
            };
            if at.translation.distance_squared(eyes.translation) >= ENTITY_REACH * ENTITY_REACH {
                return;
            }
            let target = if world.get::<crate::player::Player>(target).is_some() {
                Pointed::Player(target)
            } else if world.get::<Mob>(target).is_some() {
                Pointed::Mob(target)
            } else if world.get::<Minecart>(target).is_some() {
                Pointed::Minecart(target)
            } else if world.get::<Boat>(target).is_some() {
                Pointed::Boat(target)
            } else if world.get::<Fireball>(target).is_some() {
                Pointed::Fireball(target)
            } else {
                // Something that is not clicked on.
                return;
            };
            // The client may have used its held item up on its own copy.
            client.windows.resend();
            act(
                world,
                Action::UseEntity {
                    target,
                    attack,
                    look: eyes.rotation * Vec3::NEG_Z,
                },
            );
        }
        ClientPacket::EntityAction { state } => match state {
            1 | 2 => {
                if let Some(mut input) = world.get_mut::<PlayerMovementInput>(entity) {
                    input.sneaking = state == 1;
                }
            }
            // "Leave Bed".
            3 => {
                if let Some(mut sleep) = world.get_mut::<PlayerSleep>(entity)
                    && sleep.sleeping
                {
                    sleep.leave = true;
                }
            }
            _ => {}
        },
        ClientPacket::WindowClick {
            window,
            slot,
            button,
            action,
            shift,
            clicked,
        } => {
            let dropped = client.windows.click(
                world,
                entity,
                window,
                slot,
                button,
                action,
                shift,
                clicked,
                &mut client.out,
            );
            throw(world, entity, shared.rng, dropped);
        }
        // `handleUpdateSign`: only a sign nobody has written on yet takes
        // text, and a line with a character chat would not take reads "!?".
        ClientPacket::UpdateSign { x, y, z, lines } => {
            let mut chunks = world.resource_mut::<WorldChunks>();
            let Some(sign) = chunks.sign_at_mut(x, y, z).filter(|sign| sign.editable) else {
                return;
            };
            sign.lines = lines.map(|line| {
                if line
                    .chars()
                    .all(|c| c >= ' ' && c != '\u{a7}' && c != '\u{7f}')
                {
                    line
                } else {
                    "!?".to_owned()
                }
            });
            sign.editable = false;
            shared.signs.push(SignChange {
                dimension,
                at: IVec3::new(x, y, z),
                lines: sign.lines.clone(),
            });
            if let Some(mut persistence) =
                world.get_resource_mut::<crate::world::persistence::WorldPersistence>()
            {
                persistence.mark_dirty(ChunkPosition::from_block(x, z));
            }
        }
        ClientPacket::Transaction { window, action } => client.windows.acknowledge(window, action),
        ClientPacket::CloseWindow(_) => {
            let dropped = client.windows.close(world, entity);
            throw(world, entity, shared.rng, dropped);
        }
        // `handleRespawnPacket`: only the dead are respawned. The world
        // stands them up, and `send_state` tells the client once it has.
        ClientPacket::Respawn => {
            if world
                .get::<PlayerHealth>(entity)
                .is_some_and(|health| health.current == 0)
            {
                world
                    .entity_mut(entity)
                    .insert(ClientRespawn { requested: true });
            }
        }
        _ => {}
    }
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
    rng: &mut ItemRng,
) {
    let Some((dimension, entity)) = client.place else {
        return;
    };
    let Some(world) = host.world_mut(dimension) else {
        return;
    };
    if dimension != client.dimension {
        // `sendPlayerToOtherDimension`: the client drops its world, and
        // with it every entity and whatever window it had open.
        client.dimension = dimension;
        client.out.respawn(dimension_byte(dimension));
        client.sent.clear();
        client.shown.clear();
        client.attached.clear();
        client.reposition = true;
        client.raining = false;
        let dropped = client.windows.close(world, entity);
        throw(world, entity, rng, dropped);
        client.windows.resend();
        // The player is a new entity on this side.
        world.entity_mut(entity).insert(ClientRespawn::default());
    }
    // `recreatePlayerEntity`: the world has stood a dead player back up, at
    // their client's asking.
    let alive = world
        .get::<PlayerHealth>(entity)
        .is_some_and(|health| health.current > 0);
    if alive && client.dead {
        client.dead = false;
        client.out.respawn(dimension_byte(dimension));
        client.reposition = true;
        client.health = None;
        client.windows.resend();
    } else if !alive {
        client.dead = true;
    }
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
        // `TileEntitySign.getDescriptionPacket`, for each sign in it.
        for (index, sign) in generated.chunk.signs() {
            client.out.update_sign(
                position.x * 16 + (index % 16) as i32,
                (index / 256) as i32,
                position.z * 16 + (index / 16 % 16) as i32,
                &sign.lines,
            );
        }
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

    // The server moved the player: a teleport, a respawn, a bed. A rider is
    // carried by what they ride, which their client does for itself.
    let riding = world.get::<crate::entity::mount::Mounted>(entity).is_some();
    if riding && !client.reposition {
        client.known = transform.translation;
    } else if client.reposition
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

    // Block changes: one packet for a block alone in its chunk, and
    // `Packet52MultiBlockChange` for several.
    let mut cells = std::mem::take(&mut client.resend);
    if let Some(changed) = changes.get(&dimension) {
        cells.extend(changed);
    }
    cells.retain(|cell| {
        (0..128).contains(&cell.y)
            && client
                .sent
                .contains(&ChunkPosition::from_block(cell.x, cell.z))
    });
    cells.sort_by_key(|cell| (cell.x >> 4, cell.z >> 4, cell.x, cell.z, cell.y));
    cells.dedup();
    let chunks = world.resource::<WorldChunks>();
    for group in cells.chunk_by(|a, b| (a.x >> 4, a.z >> 4) == (b.x >> 4, b.z >> 4)) {
        let blocks: Vec<(IVec3, u8, u8)> = group
            .iter()
            .filter_map(|cell| {
                let block = chunks.block_at(cell.x, cell.y, cell.z)?;
                Some((
                    *cell,
                    block.as_u8(),
                    chunks.metadata_at(cell.x, cell.y, cell.z),
                ))
            })
            .collect();
        match blocks.as_slice() {
            [] => {}
            [(cell, block, metadata)] => {
                client
                    .out
                    .block_change(cell.x, cell.y, cell.z, *block, *metadata);
            }
            several => {
                let local: Vec<([u8; 3], u8, u8)> = several
                    .iter()
                    .map(|(cell, block, metadata)| {
                        (
                            [(cell.x & 15) as u8, cell.y as u8, (cell.z & 15) as u8],
                            *block,
                            *metadata,
                        )
                    })
                    .collect();
                client
                    .out
                    .multi_block_change(group[0].x >> 4, group[0].z >> 4, &local);
            }
        }
    }

    if ticks % 20 == 0 {
        client.out.keep_alive();
        client.out.time(time as i64);
    }
    // `Packet70Bed`: the rain starting and stopping. The Nether has none.
    if dimension == Dimension::Overworld {
        let raining = world
            .get_resource::<WorldWeather>()
            .is_some_and(|weather| weather.raining);
        if raining != client.raining {
            client.raining = raining;
            client.out.game_state(if raining { 1 } else { 2 });
        }
    }
    let dropped = client
        .windows
        .sync(world, entity, transform.translation, &mut client.out);
    throw(world, entity, rng, dropped);
    if let Some(health) = world.get::<PlayerHealth>(entity)
        && client.health != Some(health.current)
    {
        client.health = Some(health.current);
        client.out.health(i16::from(health.current));
    }
    for line in chats.get(&dimension).into_iter().flatten() {
        client.out.chat(line);
    }
}
