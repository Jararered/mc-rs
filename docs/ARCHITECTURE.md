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

## Adding a dimension

A new generator can reuse chunk storage, population-world access, noise,
lighting, persistence records, and meshing without duplicating the Overworld
implementation. Keep its algorithms in a sibling of `generation::overworld`.

This refactor does not implement the Nether or live dimension switching. The
current simulation and renderer still apply Overworld environment rules,
including sunlight, daylight, water, and freezing. Nether work must supply the
appropriate environment policies at those boundaries, and dimension switching
must partition save paths/world resources and retire in-flight jobs. The
`WorldGeneration` override selects generation at startup; it is not a runtime
transition or a dimension-aware save format.

## Verification

Tests remain under `tests/`. The existing `world` binary includes the rendering
suite from `tests/rendering/` so reorganizing it does not add another Bevy link.
Regression coverage includes pinned Beta chunks, save/resume behavior, input
focus, headless chat dispatch, headless world composition, and an alternate
generator used through real startup and background streaming jobs.
