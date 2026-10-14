# Project direction

Build a Minecraft Beta-inspired game in Rust with Bevy. The first priority is a playable, performant
recreation of the Beta experience: terrain, blocks, movement, interaction, lighting, inventory,
mobs, and the other systems needed for the game to feel right. Prefer idiomatic Rust and Bevy
implementations, and measure performance in the paths that matter most.

Compatibility with original Minecraft Beta 1.7.3 worlds and servers comes later. Keep core game data
and rendering separate so adapters can be added at explicit boundaries. Do not delay gameplay work,
or shape every internal API around the old file format, block IDs, or network protocol.

The map of the other docs is `docs/README.md`.

# Reference sources

- The decompiled Beta 1.7.3 Java source is checked in under
  `refs/mc_b1.7.3_release/1.7.3-LTS/src/minecraft/net/minecraft/{src,client}/` (client) and
  `.../minecraft_server/net/minecraft/src/` (server). It is the behavioral source of truth for
  generation, block and item behavior, recipes, and options. The Java files are large. Grep for the
  methods and fields involved, and do not read a file whole.
- `refs/mc_b1.7.3_release` is a nested git repo recorded as a gitlink with no `.gitmodules`, so it
  may be missing in a fresh clone. `refs/` is otherwise not ignored.
- `.../1.7.3-LTS/jars/world` (server, seed `-5779659068535663308`) and `.../jars/saves/New World`
  (client) are worlds the real game generated, in McRegion format.
  `tests/world/generation/beta_reference.rs` pins chunks from the server world. Both worlds ran
  briefly after generating, so gravel has fallen and springs have flowed. Tests copy a world out of
  `refs/` before opening it. Opening a world writes `session.lock`.
- `CHANGELOG.md` is the player-visible checklist to test in game, under Blocks, Items, Mobs, and
  Generation, each split into Additions, Interactions, and Fixes. Additions are grouped under a
  heading per item, block, or mob. Add a line to the Unreleased entry with every gameplay change.
  Each entry ends with `hash · date time` for the commit that made it. Take both from
  `git log -1 --format='%h · %ad' --date=format:'%Y-%m-%d %H:%M'`.
  Write `pending` until that commit exists, and replace earlier `pending` lines with the real hash
  and time whenever the file is next edited.
- What is left to build is `docs/todo/` and `ROADMAP.md`. Tick a todo when it lands, and add what
  you find missing.
- Read `docs/BLOCK_TICKS.md` before adding or changing a block that flows, falls, grows, decays, or
  reacts to its neighbors.
- Read `docs/ARCHITECTURE.md` before changing plugin composition, the world session, dimensions, or
  streaming.
- Read `docs/PERFORMANCE.md` before changing meshing, streaming, cameras, or the build profiles. The
  rules those measurements produced are below.

# Commands

- `cargo run` starts the game with unoptimized game code. Use `cargo run --release` (fat LTO) to
  judge performance, or `cargo run --profile fast` (release without LTO, incremental, output in
  `target/fast/`) while iterating. Confirm final frame times on `--release`. The feature
  `dev_dynamic_linking` builds a second copy of Bevy and was not faster here.
- `cargo run --bin server` is the dedicated server for Beta 1.7.3 clients (`src/bin/server.rs`). Add
  a flag as a field of its `clap` `Options`. Enter on stdin saves and stops it. Plain `cargo run` is
  the game (`default-run`). Both binaries link the `game` library (`src/lib.rs`). Put code there,
  never in a binary.
- `cargo test` runs every test in one binary rooted at `tests/main.rs` (`autotests = false`; the lib
  and bin harnesses are off). Filter with a path such as `world::block_ticks::`. Another test target
  would statically link Bevy again, so add a module under `tests/`
  instead. `[profile.dev]` builds game code unoptimized, dependencies at `opt-level = 3`, and `test`
  inherits that, so `cargo run` and `cargo test` share artifacts. Do not give `[profile.test]` its
  own settings.
- Bevy is built with `default-features = false` and an explicit feature list in `Cargo.toml`. Add a
  feature when a system needs it. Audio is `bevy_audio` with `vorbis` alone.
- `cargo fmt` runs on the nightly pinned in `rust-toolchain.toml` (`rustfmt` and `clippy` are
  components; `rustfmt.toml` sets `imports_granularity = "Item"`). Bump the pin deliberately.
  `cargo deny check` uses `deny.toml`.
- Build only into this checkout's `target/`. In a worktree, set `CARGO_TARGET_DIR` to that
  directory. A cold Bevy build is expensive.
- Edition 2024 is in use, including let-chains (`if let ... && let ...`).
- `examples/chunk_render.rs` (feature `render_harness`) streams terrain without the menus and can
  diff screenshots. `MC_CHUNK_QUADS=off` compares the old vertex path, `MC_CHUNK_REGIONS=off` one
  entity per section layer, and `MC_OCCLUSION=off` no cave culling. `WEATHER=rain` renders a storm.
  Run the built binary with `BEVY_ASSET_ROOT` set to the checkout, or it draws untextured.
- On macOS, Bevy compiles each new render pipeline synchronously, so the first frame of a new
  material or mesh layout can hitch. `EntityDiagnostics` adds mob tick, pathfinding, spawning, and
  posing costs to the 10 s performance log.

# Where code goes

Organize by gameplay and engine subsystem, not by `components/` and `systems/`. Add a module when a
feature needs it. Do not create empty ones.

- `src/app/` is client assembly: screens, settings, and the world session. `src/block/`,
  `src/item/`, `src/inventory/`, and `src/crafting/` are the game's data and containers.
  `src/world/` is simulation, generation, lighting, and persistence. `src/physics/`, `src/entity/`,
  and `src/player/` are bodies and the player. `src/rendering/`, `src/ui/`, and `src/audio/` are the
  client. `src/networking/` is transports. `src/chat/` is commands and history, and it runs without
  UI.
- Static per-block and per-item facts go in `data/*.ron` (`blocks`, `items`, `recipes`, `furnace`,
  `creature_models`, `sounds`), compiled in with `include_str!`. A malformed file panics with its
  position. Item names use `Item::named`: `"Name"`, `"Name:data"`, and `"block.Name"`.
- `assets/` is local reference content (the terrain atlas, GUI textures, the font). It is not
  distributable, and the game must start when it is absent. Do not add more Minecraft files there.
- `resources/` is the player's copy of Beta's sounds and music, gitignored. The game runs silent
  without it, and tests never need it.
- `tests/` mirrors `src/`. Helpers shared across subsystems are `pub(crate)`. Engine tests build a
  headless `App` with `MinimalPlugins` and `AssetPlugin` (and `MeshPlugin`), not `DefaultPlugins`.
- `settings.json`, `saves/`, and `screenshots/` are gitignored. F2 writes a screenshot.

# Rules

## Blocks, items, and block ticks

- Item IDs: a block's item reuses its Beta block ID. Standalone items start at 256. Resolve a raw ID
  through the item registry.
- `ItemStack` is a validated, nonempty stack. An empty slot is `None`.
- Keep Beta behavior in simulation data (block states, item data, the recipe book). Keep
  presentation in `src/rendering/` and `src/ui/`.
- A block ID never encodes state. Chunks store Beta's 4-bit metadata separately, and only once a
  chunk holds a nonzero value. Writing a different block resets metadata to 0, so an oriented or
  species block is placed with `set_block_with_metadata`. Metadata that changes how a block is drawn
  updates `Block::appearance_metadata` and the mesher. `Block::placed` stays `None` until the block
  has tiles and a shape. A missing tile falls back to stone.
- Block update behavior implements `BlockBehavior` under `src/world/block_ticks/behaviors/` and is
  registered in `behaviors::table()`. Behaviors read and write the world only through `TickWorld`,
  and they never touch the ECS. Drops and spawns are `TickEffect`s applied after the pass.
  Transcribe the Java, including the order of random draws and which writes notify.
- A system that writes blocks outside the tick pass takes `world::edit::WorldEdit` and calls
  `edit.changed` after writing through `edit.chunks`. Player interactions use `edit.push_event` and
  `edit.notify`. Code that holds a `World` (`WorldHost`) makes those calls itself. Writes inside the
  tick pass are relit, remeshed, and saved on their own.
- Read `docs/BLOCK_TICKS.md` before adding one.

## Saving

- Persistence has two formats, native JSON (the default) and Beta's own (`original`). Beta stores no
  biomes, scheduled ticks, or mob timers, so the original loader rebuilds climate from the seed and
  uses defaults. Blocks the game does not simulate load as air. Tile entities other than chests,
  furnaces, dispensers, note blocks, jukeboxes, signs, and spawners are dropped. Fields Beta has no
  tag for (difficulty, flying) live in a `mc-rs.json` sidecar that Beta ignores. A new world-scoped
  piece of state goes in both formats, or `original.rs` documents that it is lost.
- Saving never blocks a frame on a whole batch. The 60 s autosave, or `request_save`, starts a
  drain. Each frame `WorldPersistence::pump` snapshots at most `MAX_SNAPSHOTS_PER_FRAME` chunks
  under `SNAPSHOT_BUDGET` (the `PAUSED_` limits while the pause menu is open or the game is on a
  menu). Encoding and the file writes run on one `IoTaskPool` task. A chunk is in at most one of
  `dirty`, `saving`, and `pending`. A dirty chunk that unloads waits in `pending`, and streaming
  reloads it with `take_pending` and will not load a chunk that `is_saving`. A failed write hands
  its snapshots back: loaded chunks become dirty, and unloaded ones wait in `retry`. Each drain ends
  with one manifest write. Exit waits for the in-flight write and then saves what is left
  synchronously. `Task::drop` cancels the task, so never drop one you still need.
- A world's difficulty is stored on its manifest and copied into `GameSettings` while that world is
  loaded. The player's own option waits in `ClientDifficulty`, which is what `settings.json` keeps.
  See `docs/ARCHITECTURE.md`.

## Players

- `Player` marks every player. `LocalPlayer` marks the one this client controls. Simulation iterates
  `With<Player>`: mob targeting and despawning, spawning, random ticks and lightning, pickups,
  damage, hazards, and the night skip. Input, the camera, the HUD, rendering, chat commands, session
  travel, and the saved `player.json` ask for `With<LocalPlayer>`. A system that pairs a
  `LocalPlayer` query with other mutable queries excludes them with `Without<LocalPlayer>`, not
  `Without<Player>`.
- `entity::Flying` means "flying right now". `survival::apply_game_mode` keeps it, and
  `PlayerCombat::invulnerable`, in step with `GameMode`. Damage and mob targeting read that flag,
  not the mode.
- A player's hands are `PlayerAction`s (`player/actions.rs`). `interact_blocks` is the client half.
  `apply_player_actions` applies them to the named player and needs no window, camera, or input. A
  new thing a click does is added in `apply.rs`, and to `Action` when the client has to say
  something new. The world answers a container with `WindowOpen`. `ItemUse`, `BedUse`, and
  `ChatSubmission::sender` name their player too.
- A departure from Beta is a `GameSettings` toggle on the Features tab, with Beta's behavior at the
  off position. Wire it on the settings screen. An option that changes geometry remeshes. Time of
  day, lighting settings, and fog do not. Add a test.
- Client options live in `settings.json` (`src/app/settings.rs`). Gameplay reads `GameSettings`, not
  the file. Changes are written after `SAVE_DELAY_SECONDS` of stillness, or at exit. Loading reads
  field by field, so one unknown value falls back without resetting the rest.

## World tick

`WorldTick` (`src/world/tick.rs`) is the only 20 Hz clock. `WorldPlugin` advances it in `First`
while `AppScreen::Playing`. Do not add another accumulator, and do not use `FixedUpdate` for
simulation. The menu does not accumulate ticks.

- A frame's delta is clamped to 1 second, and at most 10 ticks are emitted. `partial()` is the
  leftover fraction in `0..1`.
- Step simulation with `ticks_this_frame()`. Sample `just_pressed` once per frame, then run
  held-button work inside the tick loop. `just_pressed` stays true for the whole frame, so reading
  it inside the loop repeats the click on every catch-up tick.
- `world_time` counts ticks since the world started. `DAY_LENGTH` is 24000. It is stored on
  `WorldManifest`.

## Sound

- The simulation asks for a sound by Beta's `SoundPool` name through `world::sound::Sounds`.
  Producers take `Option<ResMut<Sounds>>`. Only `SoundPlugin` inserts it. A block behavior calls
  `TickWorld::play_sound` (`TickEffect::Sound`) instead.
- Pitch, and a mob's roll to speak, come from `Sounds::random()` or a `Pitch` range resolved when
  the sound is queued. They do not come from a mob's RNG or from `World.rand`, so a world simulates
  the same with and without sound.
- `audio::playback` does not use Bevy's spatial audio. Falloff is Beta's linear fade, and the pan is
  equal-power. A new `SoundCategory` needs a variant, a `GameSettings` field stored flat, and a row
  in `SoundCategory::of`.
- What is not played yet is listed in `docs/todo/GENERAL.md`.

## Multiplayer

The dedicated server speaks Beta 1.7.3 protocol 14 and runs both dimensions on
`world::host::WorldHost`. The game client does not. A packet becomes a `PlayerAction`, a
`ChatSubmission`, or a component write, and what goes back is read from the world. Do not teach the
simulation about the protocol. What clients are not told, and what is still single-player, is
`docs/todo/MULTIPLAYER.md`. How a session travels, and how the host runs two dimensions, is
`docs/ARCHITECTURE.md`.

## Data and performance

- Store blocks as compact values in chunks. A world block is not a Bevy entity. Entities are for
  rendered chunks and for independently simulated objects (players, mobs, dropped items).
- Generation produces blocks, heightmaps, and per-column climate. Lighting derives light. Meshing
  reads blocks, light, and climate. Bevy renders the meshes. Grass tops sample `grasscolor.png` with
  temperature and humidity, not the biome name alone.
- An edit raycasts into world data, writes the chunk, marks it dirty, and schedules a remesh,
  including the neighboring chunks a changed block affects.
- `stream_chunks` applies finished jobs nearest first, under `APPLY_BUDGET` of main-thread time per
  frame while playing (always at least one; unlimited while unloading or on a menu). A result that
  waits keeps its in-flight slot. Compute threads run at lowered priority (`lower_thread_priority`
  in `src/main.rs`).
- Keep generation, lighting, meshing, and streaming distinct. Do the expensive independent work off
  the main thread, and apply it on the main thread. Prioritize nearby chunks.
- `clippy.toml` caps a function at 16 parameters. Group what travels together: a `SystemParam` for
  shared resources and queries, a plain struct for values, a `QueryData` for components always read
  together. Functions transcribed from Beta keep Beta's argument lists.
- Prefer bounded allocations and reusable buffers on hot paths. Profile before a more complicated
  optimization. Measure generation, meshing, and streaming changes that can cost frames.
- Do not split a voxel crate out until the server work gives a concrete reason.
- The mesher's `BlockVertex` carries raw sky and block light. The `BlockMaterial` uniform applies
  `skylight_subtracted` and smooth lighting. Time of day and lighting settings must not rebuild
  meshes. Only geometry changes do (blocks, fancy leaves). Keep `BlockVertex::color` in step with
  the shader.
- Chunk layers are 32-byte `QuadRecord`s in one GPU storage buffer (`rendering/meshing/quads.rs`,
  `rendering/chunk_quads.rs`). A layer draws a shared proxy mesh. Keep `QuadRecord::corners` in step
  with `decode_block_quad`. A quad that is not a parallelogram is two triangle records. Dropped and
  falling blocks, and a headless app, still use the packed 16-byte vertex. `MC_CHUNK_QUADS=off`
  keeps chunk layers on that path.
- Chunks render as 16×16×16 sections. An edit relights the chunks its light can reach and rebuilds
  only sections whose blocks or light changed.
- Do not mutate assets every frame. Animate with `globals.time` in shaders, pass per-entity values
  through `MeshTag`, and write animated atlas tiles from the render world
  (`rendering/textures/water.rs`).
- A layer's records are grouped by facing, and its header holds each group's count
  (`meshing::quads`). Water is never grouped: its records are sorted back to front for the camera
  (`world::streaming::translucent`).
- Section layers stay on Bevy's mesh and material path, batched into chunk regions of 8×8 chunk
  columns: one entity per region layer, with a shared proxy mesh, a `BlockMaterial`, and an
  `Aabb` (`rendering/chunk_regions.rs`, `world/streaming/regions.rs`). Its `MeshTag` names a draw
  list in the quad buffer, rebuilt on the CPU when the camera or a layer changes. Cave culling
  (`rendering/occlusion.rs`), frustum culling per section, and facing culling decide what it lists.
  Do not add another chunk draw path without measurements. `MC_CHUNK_REGIONS=off` and the vertex
  path keep one entity per section layer. `world::streaming::ChunkCulling` decides GPU versus CPU
  culling of those entities. `MC_CHUNK_CULLING=gpu|cpu` forces it.
- There is no directional or point light in the scene. `rendering/plugin.rs` keeps both shadow-map
  resources at 16 pixels. Materials are unlit. Do not turn clustering or custom-material shadow
  participation back on.
- Every camera on the window carries the same `Msaa`, from `GameSettings::msaa`, set in
  `apply_graphics_pipeline`. Do not set `Msaa` on a camera anywhere else. Do not add a camera
  without reading the Memory section of `docs/PERFORMANCE.md`.
- Textures use nearest sampling and no mipmaps. `main` sets that on `ImagePlugin`. Loaders leave
  `ImageSampler::Default` unless they need a different address mode. The terrain atlas pins
  `lod_max_clamp` to 0.

## Module boundaries

- Compose plugins in `src/app/plugin.rs`. World simulation and the chat backend must run in a
  headless app with no UI and no render assets.
- Dependencies point one way: `block`, `item`, `crafting`, `inventory`; then the simulation
  (`world`, `physics`, `entity`, `player`, the `chat` backend); then the client (`rendering`, `ui`,
  `audio`, `app`). Simulation code does not import `rendering` or `ui`. The exceptions are
  deliberate and should not grow: `world::streaming` (the client boundary), `GameSettings` reads for
  difficulty and render distance, the `AppScreen` check behind `world::tick::playing` (use that run
  condition), the player's first-person arm, block overlay and particle requests, and the wireframe
  chat command.
- `world::streaming` schedules generation, storage, light, and mesh jobs. Dimension algorithms stay
  in `world::generation`.
- New world-scoped resources and entity kinds are added to both teardowns in `drive_session`
  (leaving a world, and changing dimension). Do not hang teardown on `OnExit(Playing)`. That also
  fires when the pause menu opens Settings.
- `settings.json` is not a design document. `docs/ARCHITECTURE.md` is the composition guide.

# Working in this repo

- Implement the requested behavior completely enough to use. Prefer that over scaffolding or an API
  with no caller.
- Follow the surrounding Rust style and the Bevy version in `Cargo.toml`.
- Keep changes focused. Preserve in-progress work. Run `cargo fmt` and the tests that cover the
  change, and say what you did not run.
- A standalone mesh test may use one entity for a block. That does not set the production world
  representation.
