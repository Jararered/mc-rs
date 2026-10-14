# Architecture

How the client is put together. The rules that apply to every change are in
[AGENTS.md](../AGENTS.md). This file is the one to read before changing plugin
composition, the world session, dimensions, or streaming.

## Plugins

`app::GamePlugin` composes the client: settings, the world, rendering, the
player, UI, audio, and the session. `WorldPlugin` initializes simulation
resources and schedules the clock, block updates, and furnaces. It runs with
`MinimalPlugins`, without cameras or render assets. The clock still advances
only while `AppScreen::Playing`. A headless simulation can set that state or
drive the block-tick API itself.

`rendering::WorldRenderingPlugin` adds terrain materials, the sky and clouds,
cameras, lighting settings, falling-block visuals, and client streaming. A
client adds it beside `WorldPlugin`. Meshes, texture loaders, shaders, the
sky, and clouds live under `rendering/`. Daylight math stays in
`world::environment`, and the sky reads it through `Environment`.

`chat::ChatPlugin` owns the command registry, submission dispatch, and a
bounded history of complete messages. Producers send `ChatSubmission` (the
text, and the player who sent it). Feedback and ordinary chat appear in
`ChatHistory`. The plugin needs no window and no font. Commands run against
world and player resources. A client debug command that wants a rendering
resource takes it optionally and says when it is missing.

`ui::ChatUiPlugin` owns keyboard editing, the input buffer, wrapping, fading,
and the UI entities. `ChatSet::{Input, Dispatch, Presentation}` orders those
steps before gameplay input. `chat::ChatFocus` exposes focus without exposing
screen widgets to player controls.

`inventory::session::{InventorySession, ActiveWorkbench}` owns the open
container and the workbench inputs. `close_crafting_session` returns crafting
inputs and the carried stack to the inventory and drops what does not fit.
Player interactions and the GUI share this state. The GUI owns pointer
handling and how long the screen lives. `rendering::appearance` and
`rendering::icons` provide the item visuals both the widgets and the world
sprites use.

## Dependencies

They point one way, as [AGENTS.md](../AGENTS.md) states: data and containers,
then simulation, then the client. Simulation does not import `rendering` or
`ui`. The exceptions listed there should not grow.

`world::streaming` is the client boundary that is allowed to. It snapshots
shared world data, schedules generation, storage, light, and mesh workers, and
publishes meshes. Concrete dimension algorithms stay in `world::generation`.
A mesh job also records each section's visibility graph for cave culling. With
the quad buffer, streaming uploads a section layer's records and keeps where
they are; chunk regions (`world::streaming::regions`) list them for the
camera each frame and own one entity per region layer, so leaving a world or
changing dimension despawns them with the chunk meshes in
`WorldStreaming::despawn_rendered`.
The server still lights through these mesh jobs, so a hosted dimension builds
meshes nobody draws. Lighting without meshing is listed in
[todo/MULTIPLAYER.md](todo/MULTIPLAYER.md).

## World data and generation

`world::chunk` holds compact blocks, metadata, containers, pending ticks,
`GeneratedChunk` records, heightmaps, and the loaded chunk collection. Column
climate and biome identity live in `world::biome`, independent of the
generator. `world::lighting`, `world::block_ticks`, `world::tick`, and
`world::environment` are light, block updates, the shared clock, and Overworld
daylight. Saving and loading are `world::persistence`, in two formats. See
that module and the saving rules in [AGENTS.md](../AGENTS.md).

`ChunkGenerator` is shared across workers as `Arc<dyn ChunkGenerator>`. It
builds a base chunk alone, then populates that chunk together with its `+x`,
`+z`, and `+x+z` neighbors once they exist, in `population_footprint` order,
writing into all four. Population preserves the source chunk's other flags and
sets its own. Streaming schedules disjoint footprints and meshes a chunk only
once every population pass that contributes to it has finished.

A client may insert `WorldGeneration::new(generator)` before startup to choose
a backend. The default constructs `OverworldGenerator` with the saved seed.
The override covers the initial spawn neighborhood and the async jobs. A
loaded chunk still wins over generation. Offline tools can call
`generate_area(&generator, center, radius)` without a Bevy app. The Overworld
convenience API keeps a bounded base cache. Streaming's own base jobs stay
uncached.

## World session

The game starts with no world. `GamePlugin` uses `PersistencePlugin::deferred()`,
which skips the startup systems that load one (`setup_persistence`,
`setup_streaming`, `spawn_player`). Those are gated by
`persistence::starts_with_world`, so a headless test that does not defer still
loads a world at startup. `app/session.rs` owns the lifecycle through
`WorldSession`. `stream_chunks` and `flush_persistence` run only while
`WorldStreaming` and `WorldPersistence` exist.

`request_load(WorldChoice)` opens or creates the `WorldStorage`, inserts
`PendingWorld`, and a chained system set installs it. `activate_pending_world`
sets the clock, weather, difficulty, and `WorldPersistence`, starts the spawn
area on the compute pool (`start_spawn_area`), and stays in `Phase::Loading`
until `setup_streaming` picks the result up. That frame spawns the player and
switches to `AppScreen::Playing`. `activate_pending_world` must run once per
load: a second pass finds the storage taken and installs
`WorldPersistence::disabled()`. A world loaded at startup still builds its
spawn area inline.

`request_leave()` (Quit to title) unloads in two steps. Settling:
`WorldStreaming::halt` stops new generation, population, and first-mesh jobs,
and `WorldPersistence::begin_closing` stops `WorldTick`. The session waits for
the generation and population jobs in flight, which still hand chunks back.
Saving: only then does it call `request_final_save`, so nothing changes behind
a chunk's last snapshot. It waits until `WorldPersistence::is_idle`, with
nothing unsaved and no `save_error`. A drain writes each chunk once and ends
even when a write failed, so a leftover is saved again (a failure every
`SAVE_RETRY_SECONDS`) instead of being dropped. `WorldSession::notice` carries
the saving, save-failure, or could-not-open text. Teardown then despawns chunk
meshes, mobs, items, projectiles, falling blocks, the player, and the sky rig,
and resets world resources. Add any new world-scoped resource or entity kind
to `drive_session`'s teardown. Do not hang it on `OnExit(Playing)`, which
also fires for the pause menu's trip through Settings.

A world's difficulty is stored on its manifest (`WorldManifest::difficulty`,
`None` for an older world) and copied into `GameSettings` when it loads. While
that world stands in, the player's own option waits in `ClientDifficulty`,
which is what `settings.json` keeps and what teardown restores. A change on
the settings screen is written back to the manifest with each save.

## Dimensions

One save holds the Overworld and the Nether. In the game, only
`ActiveDimension` is loaded: `WorldChunks`, `LightCache`, `BlockTicks`, mobs,
and items all belong to it. World time, weather, and difficulty are shared, as
Beta shares `WorldInfo`. Weather does not advance in the Nether.

`Dimension` carries Beta's `WorldProvider` rules: sky, weather, ambient light,
celestial angle, lava reach, respawning, the 8:1 coordinate scale, and the
`DIM-1` save folder. Systems that light or shade read
`world::dimension::Environment` (`skylight_subtracted`, `ambient_light`,
`is_raining`, `celestial_angle`) instead of calling `world::environment`
directly. The Nether has no sky light. Mesh jobs clear the sky channel
(`Skylight::without_sky`, from `LightCache::has_sky`),
`Dimension::skylight_subtracted` is 15 so column estimates are block light
alone, and `BlockShadingSettings::ambient` raises the darkest brightness to
0.1.

Nether chunks are saved under `DIM-1/` in both formats (`DIM-1/regions/`
native, `DIM-1/region/*.mcr` in Beta's). Each `WorldPersistence` owns the
dimension it loads and saves, so two can share one `WorldStorage`. The one a
world was opened with also writes the manifest and the player.
`WorldStorage::load_chunk_in` and `save_chunks_in` name a dimension. The plain
`load_chunk` and `save_chunks` mean the Overworld. `StoredPlayer::dimension`
(Beta's `Dimension` tag) decides where a world reopens.

`generation::nether::NetherGenerator` is a sibling of
`generation::overworld` and reuses chunk storage, `PopulationWorld`, noise,
lighting, persistence records, and meshing. `setup_streaming` picks the
generator from `ActiveDimension` unless a `WorldGeneration` override is
present. Nether population is seeded per chunk. Beta never reseeds `hellRNG`
inside `populate`, so its glowstone, fire, and lava falls depend on chunk load
order. Terrain, surface, and caves match Beta and are checked against the
server world's `DIM-1`.

`WorldSession::request_travel(Travel::Portal | Travel::Respawn)` changes
dimension without unloading the world. A respawn from the Overworld to a bed
whose chunks are not loaded goes the same way, Overworld to Overworld. It
settles and does a final save as a leave does, then a partial teardown that
keeps the player, `WorldPersistence`, and the sky rig. `Phase::Arriving` runs
while a task runs `world::portal` (Beta's `Teleporter`: the nearest portal
within 128 blocks, or a new one within 16) over a `PortalArea` of chunks held
outside the live world. The chunks a new portal was built in are handed to
`WorldChunks` before streaming restarts. Then `admit_arrival`,
`setup_streaming`, and `finish_arrival`. `WorldPersistence::set_dimension` is
legal only once it is idle with nothing unsaved. Its bookkeeping is keyed by
chunk position alone. Add any new dimension-scoped state to that partial
teardown as well as the full one.

The player's portal state is `player::portal::PortalTravel` (Beta's
`inPortal`, `timeInPortal`, `timeUntilPortal`). `physics::touch_blocks` is
`onEntityCollidedWithBlock` for the player: a portal charges that state, and
soul sand slows. A rider's box still touches blocks, and a rider in a portal
is put off its vehicle (`tick_portal_travel`, and `WorldHost::tick_portals`
on the server). Mobs and items do not use portals, as in Beta 1.7.3.

`Travel::Respawn` from the Nether runs the teleporter for the dead player
first, as Beta's `respawn` runs `usePortal`: the portal it finds or builds
near the death point, scaled by eight, is kept, and the player is then put at
the bed or the spawn.

`world::host::WorldHost` is the other way to run two dimensions, and it is
what the dedicated server uses. One headless Bevy world per loaded dimension,
as Beta runs a `WorldServer` each over a shared `WorldInfo`. Every system
keeps reading the single-dimension resources of the world it is in. The host
hands each world the shared clock and weather before its update, and each has
its own `WorldPersistence` over the one `WorldStorage`. Players are the only
thing that crosses, as a `StoredPlayer` placed by the same teleporter. A
dimension with no player settles, saves, and unloads. The game client still
plays one dimension through the session above. The client's world becoming
separate from the server's is the split listed in
[todo/MULTIPLAYER.md](todo/MULTIPLAYER.md).

A third dimension would add a `Dimension` variant with its policies, a
generator module, and a save folder. The session's travel path and the storage
do not assume there are only two.

## Verification

Tests stay under `tests/`, as one binary (`tests/main.rs`) whose modules
mirror `src/`. Changing the paths in this file should still leave these
covered: pinned Beta chunks from both dimensions, portal frames, the
teleporter, dimension travel through a real session, save and resume, input
focus, headless chat dispatch, a headless world, and an alternate generator
used through real startup and background streaming jobs.
