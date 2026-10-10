//! Both dimensions of one world, simulated at once.
//!
//! Beta's server runs a `WorldServer` per dimension over a shared `WorldInfo`
//! (`WorldServerMulti`). [`WorldHost`] does the same with one headless Bevy
//! world per loaded dimension: each keeps the single-dimension resources
//! every system already reads ([`WorldChunks`], `LightCache`, [`BlockTicks`],
//! [`ActiveDimension`]), and the host owns what they share: the clock, the
//! weather, the difficulty and the [`WorldStorage`].
//!
//! Only players cross between dimensions, as in Beta 1.7.3, so the only
//! traffic between two of these worlds is a [`StoredPlayer`] record.
//!
//! The game itself still plays in a single world through `app::session`. This
//! is the server half of the client/server split, and has no client yet.

use std::collections::HashSet;
use std::io;
use std::sync::Arc;

use bevy::math::DVec3;
use bevy::prelude::*;

use crate::entity::EntitySize;
use crate::player::Player;
use crate::player::PlayerName;
use crate::player::portal::PortalTravel;
use crate::player::spawn_player_body;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::dimension::ActiveDimension;
use crate::world::dimension::Dimension;
use crate::world::generation::ChunkGenerator;
use crate::world::generation::generate_area;
use crate::world::generation::nether::NetherGenerator;
use crate::world::generation::overworld::OverworldGenerator;
use crate::world::persistence::StoredPlayer;
use crate::world::persistence::WorldPersistence;
use crate::world::persistence::WorldStorage;
use crate::world::portal;
use crate::world::portal::PortalArea;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;
use crate::world::weather::WorldWeather;

/// Marks a world as one dimension of a [`WorldHost`]: its [`WorldTick`] is set
/// from outside each frame, and it builds no spawn area of its own.
#[derive(Resource)]
pub struct Hosted;

/// `Entity.yOffset` for the player, twice: see `app::session`.
const PORTAL_Y_DRIFT: f64 = 2.0 * 1.62;
/// Chunks around the arrival point generated before a portal is built.
const ARRIVAL_CHUNK_RADIUS: i32 = 2;
/// The world spawn, where a player with no record starts.
const WORLD_SPAWN: Vec3 = Vec3::new(8.5, 64.0, 8.5);

/// Where a dimension is in unloading once its last player has left.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Open,
    /// Waiting for generation and population jobs to hand their chunks back.
    Settling,
    /// Waiting for the final save to reach disk.
    Saving,
}

struct Hosting {
    dimension: Dimension,
    app: App,
    stage: Stage,
}

/// One world, with each dimension that has a player in it loaded.
pub struct WorldHost {
    storage: Arc<WorldStorage>,
    /// Adds the simulation plugins to a new dimension's app.
    build: Box<dyn Fn(&mut App)>,
    autosave_seconds: f32,
    /// `WorldInfo`'s time, advanced once a frame for every dimension.
    clock: WorldTick,
    /// The difficulty every dimension runs at.
    difficulty: Option<crate::world::difficulty::Difficulty>,
    /// `WorldInfo`'s weather. Only the Overworld steps it.
    weather: WorldWeather,
    worlds: Vec<Hosting>,
    /// Players whose trip is waiting for the far side to be free.
    waiting: Vec<(Dimension, Entity)>,
    since_level_save: f32,
}

impl WorldHost {
    /// Host `storage`. `build` adds the simulation to each dimension's app;
    /// the host adds the clock, saving and the dimension itself.
    pub fn new(
        storage: WorldStorage,
        autosave_seconds: f32,
        build: impl Fn(&mut App) + 'static,
    ) -> Self {
        let manifest = storage.manifest();
        let mut clock = WorldTick::default();
        clock.set_world_time(manifest.world_time);
        Self {
            storage: Arc::new(storage),
            build: Box::new(build),
            autosave_seconds,
            clock,
            difficulty: manifest.difficulty,
            weather: manifest.weather,
            worlds: Vec::new(),
            waiting: Vec::new(),
            since_level_save: 0.0,
        }
    }

    pub fn storage(&self) -> &Arc<WorldStorage> {
        &self.storage
    }

    /// Run every dimension at `difficulty` instead of the one the world was
    /// saved with. The saved one is left as it is.
    pub fn set_difficulty(&mut self, difficulty: crate::world::difficulty::Difficulty) {
        self.difficulty = Some(difficulty);
        for hosting in &mut self.worlds {
            if let Some(mut settings) = hosting
                .app
                .world_mut()
                .get_resource_mut::<crate::app::settings::GameSettings>()
            {
                settings.difficulty = difficulty;
            }
        }
    }

    pub fn world_time(&self) -> u64 {
        self.clock.world_time()
    }

    /// The dimensions that are loaded, including one still saving to unload.
    pub fn loaded(&self) -> Vec<Dimension> {
        self.worlds.iter().map(|world| world.dimension).collect()
    }

    pub fn world(&self, dimension: Dimension) -> Option<&World> {
        self.hosting(dimension).map(|world| world.app.world())
    }

    pub fn world_mut(&mut self, dimension: Dimension) -> Option<&mut World> {
        self.worlds
            .iter_mut()
            .find(|world| world.dimension == dimension)
            .map(|world| world.app.world_mut())
    }

    fn hosting(&self, dimension: Dimension) -> Option<&Hosting> {
        self.worlds
            .iter()
            .find(|world| world.dimension == dimension)
    }

    /// Load `dimension` if it is not, and keep it if it was unloading.
    fn ensure(&mut self, dimension: Dimension) -> &mut Hosting {
        if let Some(index) = self
            .worlds
            .iter()
            .position(|world| world.dimension == dimension)
        {
            let hosting = &mut self.worlds[index];
            if hosting.stage != Stage::Open {
                hosting.stage = Stage::Open;
                let world = hosting.app.world_mut();
                if let Some(mut persistence) = world.get_resource_mut::<WorldPersistence>() {
                    persistence.resume();
                }
                if let Some(mut streaming) = world.get_resource_mut::<WorldStreaming>() {
                    streaming.resume();
                }
            }
            return &mut self.worlds[index];
        }
        let mut app = App::new();
        (self.build)(&mut app);
        crate::world::persistence::add_saving(&mut app);
        let mut clock = self.clock.clone();
        clock.idle();
        app.insert_resource(Hosted)
            .insert_resource(ActiveDimension(dimension))
            .insert_resource(clock)
            .insert_resource(self.weather.clone())
            .insert_resource(WorldPersistence::for_dimension(
                Arc::clone(&self.storage),
                dimension,
                self.autosave_seconds,
            ));
        let world = app.world_mut();
        if let Some(mut ticks) = world.get_resource_mut::<BlockTicks>() {
            // Chunks carry their pending ticks as delays from now.
            let previous = ticks.time();
            ticks.rebase_time(previous, self.clock.world_time());
            ticks.set_dimension(dimension);
        }
        if let Some(difficulty) = self.difficulty
            && let Some(mut settings) =
                world.get_resource_mut::<crate::app::settings::GameSettings>()
        {
            settings.difficulty = difficulty;
        }
        self.worlds.push(Hosting {
            dimension,
            app,
            stage: Stage::Open,
        });
        // The Overworld runs first each frame: it is the one that steps the
        // weather and can skip the night.
        self.worlds
            .sort_by_key(|world| world.dimension != Dimension::Overworld);
        self.worlds
            .iter_mut()
            .find(|world| world.dimension == dimension)
            .unwrap()
    }

    /// Where the player called `name` is.
    pub fn find(&mut self, name: &str) -> Option<(Dimension, Entity)> {
        self.worlds.iter_mut().find_map(|hosting| {
            let world = hosting.app.world_mut();
            let mut players = world.query::<(Entity, &PlayerName)>();
            players
                .iter(world)
                .find(|(_, player)| player.0 == name)
                .map(|(entity, _)| (hosting.dimension, entity))
        })
    }

    /// Put the player called `name` into the world: where their record left
    /// them, or at the world spawn the first time.
    pub fn join(&mut self, name: &str) -> (Dimension, Entity) {
        if let Some(found) = self.find(name) {
            return found;
        }
        let saved = self.storage.load_named_player(name);
        let dimension = saved.as_ref().map_or(Dimension::Overworld, |p| p.dimension);
        let fallback =
            Transform::from_translation(WORLD_SPAWN + Vec3::Y * EntitySize::PLAYER.y_offset);
        let entity = self.spawn(dimension, name, saved.as_ref(), fallback, None);
        (dimension, entity)
    }

    fn spawn(
        &mut self,
        dimension: Dimension,
        name: &str,
        saved: Option<&StoredPlayer>,
        fallback: Transform,
        portal: Option<PortalTravel>,
    ) -> Entity {
        let world = self.ensure(dimension).app.world_mut();
        let entity = spawn_player_body(&mut world.commands(), saved, fallback);
        world.flush();
        let mut player = world.entity_mut(entity);
        player.insert(PlayerName(name.to_owned()));
        if let Some(portal) = portal {
            player.insert(portal);
        }
        entity
    }

    /// Save the player called `name` and take them out of the world.
    pub fn leave(&mut self, name: &str) -> io::Result<()> {
        let Some((dimension, entity)) = self.find(name) else {
            return Ok(());
        };
        self.waiting
            .retain(|waiting| *waiting != (dimension, entity));
        let world = self.world_mut(dimension).unwrap();
        let record = StoredPlayer::of(world.entity(entity), dimension);
        crate::entity::mount::detach(&mut world.commands(), entity);
        world.flush();
        world.despawn(entity);
        match record {
            Some(record) => self.storage.save_named_player(name, &record),
            None => Ok(()),
        }
    }

    /// Write every player's record and the manifest.
    pub fn save_level(&mut self) -> io::Result<()> {
        for hosting in &mut self.worlds {
            let world = hosting.app.world_mut();
            let mut players = world.query::<(EntityRef, &PlayerName)>();
            for (entity, name) in players.iter(world) {
                if let Some(record) = StoredPlayer::of(entity, hosting.dimension) {
                    self.storage.save_named_player(&name.0, &record)?;
                }
            }
        }
        self.storage.set_world_time(self.clock.world_time());
        self.storage.set_weather(&self.weather);
        self.storage.save_level()
    }

    /// Run one frame of every loaded dimension, `delta_secs` of world time on.
    pub fn update(&mut self, delta_secs: f32) {
        self.clock.advance(delta_secs);
        let ticks = self.clock.ticks_this_frame();
        for hosting in &mut self.worlds {
            let world = hosting.app.world_mut();
            // A dimension saving to unload holds still, as a closing world's
            // clock does.
            let mut clock = self.clock.clone();
            if hosting.stage != Stage::Open {
                clock.idle();
            }
            world.insert_resource(clock);
            world.insert_resource(self.weather.clone());
            hosting.app.update();
            if hosting.dimension == Dimension::Overworld && hosting.stage == Stage::Open {
                let world = hosting.app.world();
                // Sleeping moves the shared clock on to morning.
                let time = world.resource::<WorldTick>().world_time();
                if time != self.clock.world_time() {
                    self.clock.set_world_time(time);
                }
                if let Some(weather) = world.get_resource::<WorldWeather>() {
                    self.weather = weather.clone();
                }
            }
        }
        self.tick_portals(ticks);
        self.move_waiting();
        self.unload_empty();

        self.since_level_save += delta_secs;
        if self.since_level_save >= self.autosave_seconds {
            self.since_level_save = 0.0;
            if let Err(error) = self.save_level() {
                warn!("Failed to save the world's players and manifest: {error}");
            }
        }
    }

    /// `EntityPlayerMP.onLivingUpdate`'s portal block for every player: the
    /// tick a charge completes, they are queued for the other dimension.
    fn tick_portals(&mut self, ticks: u32) {
        if ticks == 0 {
            return;
        }
        for hosting in &mut self.worlds {
            let world = hosting.app.world_mut();
            // No physics runs here, so a player's contact with a portal
            // block is looked for where the client put them.
            let mut bodies =
                world.query_filtered::<(Entity, &Transform, &mut PortalTravel), With<Player>>();
            let contacts: Vec<(Entity, Vec3)> = bodies
                .iter(world)
                .map(|(entity, transform, _)| (entity, transform.translation))
                .collect();
            for (entity, translation) in contacts {
                let chunks = world.resource::<WorldChunks>();
                let aabb = EntitySize::PLAYER.aabb(translation);
                let mut charge = world
                    .get::<PortalTravel>(entity)
                    .copied()
                    .unwrap_or_default();
                crate::physics::touch_portals(aabb, chunks, &mut charge);
                *world.get_mut::<PortalTravel>(entity).unwrap() = charge;
            }
            let mut players = world.query_filtered::<(Entity, &mut PortalTravel), With<Player>>();
            for (entity, mut portal) in players.iter_mut(world) {
                // Physics reports contact once for the frame's ticks.
                let touching = portal.touching();
                for _ in 0..ticks {
                    if touching {
                        portal.set_in_portal();
                    }
                    if portal.tick() {
                        self.waiting.push((hosting.dimension, entity));
                    }
                }
            }
        }
    }

    /// Send the player called `name` to the other dimension, as a charged
    /// portal does.
    pub fn travel(&mut self, name: &str) {
        if let Some(found) = self.find(name)
            && !self.waiting.contains(&found)
        {
            self.waiting.push(found);
        }
    }

    fn move_waiting(&mut self) {
        for (source, entity) in std::mem::take(&mut self.waiting) {
            if !self.transfer(source, entity) {
                self.waiting.push((source, entity));
            }
        }
    }

    /// `ServerConfigurationManager.sendPlayerToOtherDimension`. Returns false
    /// when the far side cannot take them this frame.
    fn transfer(&mut self, source: Dimension, entity: Entity) -> bool {
        let target = match source {
            Dimension::Overworld => Dimension::Nether,
            Dimension::Nether => Dimension::Overworld,
        };
        let Some(world) = self.world(source) else {
            return true;
        };
        let Ok(player) = world.get_entity(entity) else {
            return true;
        };
        let Some(mut record) = StoredPlayer::of(player, target) else {
            return true;
        };
        let name = player.get::<PlayerName>().cloned();
        let portal = player.get::<PortalTravel>().copied();
        let (x, z) = source.scale_position_to(target, f64::from(record.x), f64::from(record.z));
        let at = DVec3::new(x, f64::from(record.y) + PORTAL_Y_DRIFT, z);
        // Beta's teleporter draws the rotation it tries first from an
        // unseeded random.
        let rotation = self.clock.world_time() as u32 ^ entity.index_u32();

        let storage = Arc::clone(&self.storage);
        let generator: Arc<dyn ChunkGenerator> = match target {
            Dimension::Overworld => Arc::new(OverworldGenerator::new(storage.seed())),
            Dimension::Nether => Arc::new(NetherGenerator::new(storage.seed())),
        };
        let far = self.ensure(target).app.world_mut();
        let Some(feet) = place_in_portal(far, &storage, &*generator, target, at, rotation) else {
            return false;
        };

        record.x = x as f32;
        record.z = z as f32;
        if let Some(feet) = feet {
            record.x = feet.x as f32;
            record.y = feet.y as f32 + EntitySize::PLAYER.y_offset;
            record.z = feet.z as f32;
        }
        // `setLocationAndAngles(x, y, z, rotationYaw, 0)`.
        record.pitch = 0.0;
        // A fall or a fire does not follow them through.
        record.fall_distance = 0.0;

        let near = self.world_mut(source).unwrap();
        crate::entity::mount::detach(&mut near.commands(), entity);
        near.flush();
        near.despawn(entity);
        let arrived = self.spawn(
            target,
            name.as_ref().map_or("", |name| name.0.as_str()),
            Some(&record),
            Transform::default(),
            portal,
        );
        if name.is_none() {
            self.world_mut(target)
                .unwrap()
                .entity_mut(arrived)
                .remove::<PlayerName>();
        }
        true
    }

    /// A dimension nobody is in settles, saves and unloads, in the order a
    /// world being left does.
    fn unload_empty(&mut self) {
        let waiting = &self.waiting;
        self.worlds.retain_mut(|hosting| {
            let world = hosting.app.world_mut();
            let occupied = world
                .query_filtered::<(), With<Player>>()
                .iter(world)
                .next()
                .is_some()
                || waiting
                    .iter()
                    .any(|(source, _)| *source == hosting.dimension);
            match hosting.stage {
                Stage::Open if occupied => {}
                Stage::Open => {
                    if let Some(mut streaming) = world.get_resource_mut::<WorldStreaming>() {
                        streaming.halt();
                    }
                    if let Some(mut persistence) = world.get_resource_mut::<WorldPersistence>() {
                        persistence.begin_closing();
                    }
                    hosting.stage = Stage::Settling;
                }
                Stage::Settling => {
                    let settled = world.get_resource::<WorldStreaming>().is_none_or(|s| {
                        s.generating_job_count() == 0 && s.populating_job_count() == 0
                    });
                    if settled {
                        world
                            .resource_mut::<WorldPersistence>()
                            .request_final_save();
                        hosting.stage = Stage::Saving;
                    }
                }
                Stage::Saving => {
                    let mut persistence = world.resource_mut::<WorldPersistence>();
                    if persistence.is_idle() {
                        if !persistence.has_unsaved_chunks() && persistence.save_error().is_none() {
                            return false;
                        }
                        // A drain writes each chunk once and ends even when a
                        // write failed, so what is left is saved again.
                        persistence.request_final_save();
                    }
                }
            }
            true
        });
    }
}

/// `Teleporter.placeInPortal` in a dimension that is running.
///
/// The teleporter works on a [`PortalArea`] that owns its chunks, so the
/// loaded ones are lifted out of [`WorldChunks`] for the search and put back
/// before anything else runs; chunks that are not loaded come from the save
/// or the generator, as they do for a dimension that is not loaded at all.
///
/// Returns `None` when a chunk the new portal would be built in is in a
/// streaming job's hands, to be tried again next frame; otherwise where the
/// player's feet go, if the teleporter stood them anywhere.
fn place_in_portal(
    world: &mut World,
    storage: &WorldStorage,
    generator: &dyn ChunkGenerator,
    dimension: Dimension,
    entity: DVec3,
    rotation: u32,
) -> Option<Option<DVec3>> {
    // An existing portal: among the loaded chunks, and the saved ones that
    // hold a portal block.
    let search = portal::chunks_within(entity.x, entity.z, portal::SEARCH_RADIUS, 0);
    let mut area = PortalArea::default();
    let mut live = HashSet::new();
    for &position in &search {
        if let Some(chunk) = world.resource_mut::<WorldChunks>().remove(position) {
            area.insert(position, chunk);
            live.insert(position);
        } else if let Some(chunk) = storage
            .load_chunk_in(dimension, position)
            .filter(portal::has_portal)
        {
            area.insert(position, chunk);
        }
    }
    let exit = portal::find_exit(&area, entity);
    let (chunks, _) = area.into_parts();
    for (position, chunk) in chunks {
        if live.contains(&position) {
            world.resource_mut::<WorldChunks>().insert(position, chunk);
        }
    }
    if exit.is_some() {
        return Some(exit);
    }

    // A new one, in real terrain.
    let center = ChunkPosition::from_block(entity.x.floor() as i32, entity.z.floor() as i32);
    let mut generated = generate_area(generator, center, ARRIVAL_CHUNK_RADIUS);
    if world
        .get_resource::<WorldStreaming>()
        .is_some_and(|streaming| {
            generated
                .keys()
                .any(|position| streaming.is_busy(*position))
        })
    {
        return None;
    }
    let mut live = HashSet::new();
    let mut saved = HashSet::new();
    for (position, chunk) in &mut generated {
        if let Some(loaded) = world.resource_mut::<WorldChunks>().remove(*position) {
            *chunk = loaded;
            live.insert(*position);
        } else if let Some(stored) = storage.load_chunk_in(dimension, *position) {
            *chunk = stored;
            saved.insert(*position);
        }
    }
    let mut area = PortalArea::new(generated);
    let feet = portal::place_in_portal(&mut area, entity, rotation);
    let (chunks, changes) = area.into_parts();
    let edited: HashSet<_> = changes
        .iter()
        .map(|change| ChunkPosition::from_block(change.position.x, change.position.z))
        .collect();
    for (position, chunk) in chunks {
        if live.contains(&position) {
            world.resource_mut::<WorldChunks>().insert(position, chunk);
        } else {
            world.resource_scope(|world, mut chunks: Mut<WorldChunks>| {
                world.resource_scope(|world, mut ticks: Mut<BlockTicks>| {
                    crate::world::streaming::admit_chunk(
                        &mut world.commands(),
                        &mut chunks,
                        Some(&mut ticks),
                        position,
                        chunk,
                    );
                });
            });
            world.flush();
        }
        if (!live.contains(&position) && !saved.contains(&position)) || edited.contains(&position) {
            world
                .resource_mut::<WorldPersistence>()
                .mark_dirty(position);
        }
    }
    for change in changes {
        let at = change.position;
        world.resource_mut::<BlockTicks>().block_changed(
            at,
            change.previous,
            change.previous_metadata,
        );
        if let Some(mut streaming) = world.get_resource_mut::<WorldStreaming>() {
            streaming.request_block_update(at.x, at.y, at.z);
        }
    }
    Some(feet)
}
