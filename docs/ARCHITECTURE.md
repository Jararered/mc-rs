# Module boundaries

Gameplay state belongs to its subsystem. UI screens read state, translate input
into requests, and build widgets. Rendering reads world and item data to build
meshes and materials. `app::GamePlugin` composes the client plugins.

## Chat and inventory

`chat::ChatPlugin` owns the command registry, submission dispatch, and bounded
history of complete messages. Any producer can write `ChatSubmission(String)`;
feedback and ordinary chat appear in `ChatHistory`. Neither a window nor a font
is required. Commands execute against world/player resources; client debug
commands such as wireframe use optional rendering resources and report when
those resources are unavailable.

`ui::ChatUiPlugin` owns keyboard editing, the input buffer, wrapping, fading,
and UI entities. `ChatSet::{Input, Dispatch, Presentation}` orders those steps
before gameplay input. `chat::ChatFocus` exposes focus and suppression through
the closing frame without exposing screen widgets to player controls.

`inventory::session::{InventorySession, ActiveWorkbench}` owns the current
container and workbench inputs. `close_crafting_session` returns crafting and
carried stacks to inventory and drops overflow. Player interactions and the GUI
use this same state. The GUI still owns pointer handling and screen lifetime.

`rendering::appearance` and `rendering::icons::ItemIconsPlugin` provide shared
item visuals and atlas assets used by inventory widgets and world item sprites.
UI owns only its overlays, labels, and layout.

## World data and generation

- `world::chunk`: compact blocks, metadata, containers, pending ticks, shared
  `GeneratedChunk` records, heightmaps, and the loaded chunk collection.
- `world::biome`: stored per-column climate and biome identities.
- `world::generation`: `ChunkGenerator`, the shared population footprint and
  area-generation helper, reusable noise/math, and `PopulationWorld` access.
- `world::generation::overworld`: Beta terrain, surface, caves, climate sampling,
  and decoration, implemented by `OverworldGenerator`.
- `world::lighting`, `world::block_ticks`, `world::tick`, and
  `world::environment`: lighting data, simulation, the shared clock, and
  deterministic Overworld daylight math.
- `world::persistence`: the existing save format and saving lifecycle.

`ChunkGenerator` is shared across workers as `Arc<dyn ChunkGenerator>`. It
produces base chunks and populates a source together with its +x, +z, and +x+z
neighbors, in `population_footprint` order. Population preserves other source
flags and sets its own flag. Streaming schedules disjoint footprints and waits
for every contributing population pass before meshing a neighborhood.

A client may insert `WorldGeneration::new(generator)` before startup to choose
a backend. The default constructs `OverworldGenerator` with the saved world
seed. The override is used for both the initial spawn neighborhood and async
base/population jobs. Loaded chunks still take precedence over generation.
Offline tools can call `generate_area(&generator, center, radius)` without
building a Bevy app. The Overworld convenience API keeps its bounded base cache;
streaming base jobs retain their original uncached path.

## Client composition

`WorldPlugin` initializes simulation resources and schedules ticks, block updates,
and furnaces. It can run with `MinimalPlugins`, without cameras or render assets.
The clock still advances only while `AppScreen::Playing`; a headless simulation
can manage that state or drive the block-tick API explicitly.

`rendering::WorldRenderingPlugin` adds terrain materials, sky/clouds, cameras,
lighting settings, falling-block visuals, and client streaming. Add it alongside
`WorldPlugin` in a client. `world::streaming` is the integration boundary that
snapshots shared data, schedules generation/storage/light/mesh workers, and
publishes meshes. Section invalidation and background save budgets are unchanged.

Meshes, texture loaders, shaders, sky, and clouds live under `rendering/`.
Simulation daylight calculations stay outside the sky renderer, and
render-specific block-state equivalence lives beside the mesher.

## Dimensions

A save holds the Overworld and the Nether, and one of them is loaded at a
time. `world::dimension::Dimension` carries Beta's `WorldProvider` rules (sky,
weather, ambient light, celestial angle, lava reach, respawning, the 8:1
coordinate scale, the `DIM-1` save folder), and the `ActiveDimension` resource
says which is loaded. Systems that light or shade read the
`world::dimension::Environment` system parameter instead of calling the
Overworld daylight functions in `world::environment` directly.

`generation::nether::NetherGenerator` is a sibling of `generation::overworld`
and reuses chunk storage, `PopulationWorld`, noise, lighting, persistence
records, and meshing. `setup_streaming` picks the generator from
`ActiveDimension` unless a `WorldGeneration` override is present.

Changing dimension is owned by `app::session` (`WorldSession::request_travel`).
It settles and saves like a leave, drops only the dimension's own state,
points `WorldStorage` at the other dimension, and runs `world::portal` (Beta's
`Teleporter`) on a background task over chunks held outside the live world.
The chunks a new portal was built in are handed to `WorldChunks` before
streaming restarts around the player. `WorldPersistence`'s bookkeeping is
keyed by chunk position alone, so the storage's dimension only changes while
it is idle with nothing unsaved.

A third dimension would add a `Dimension` variant with its policies, a
generator module, and a save folder; the session's travel path and the
storage do not assume there are only two.

## Verification

Tests remain under `tests/`, as one binary (`tests/main.rs`) whose modules mirror
`src/`, so reorganizing them never adds another Bevy link.
Regression coverage includes pinned Beta chunks from both dimensions, portal
frames, the teleporter, dimension travel through a real session, save/resume behavior, input
focus, headless chat dispatch, headless world composition, and an alternate
generator used through real startup and background streaming jobs.
