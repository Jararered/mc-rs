# Project direction

Build a Minecraft Beta-inspired game in Rust with Bevy. The first priority is a playable, performant recreation of the Beta experience: terrain, blocks, movement, interaction, lighting, inventory, mobs, and the other systems needed for the game to feel right. Prefer idiomatic Rust and Bevy implementations, and measure performance in the paths that matter most.

Compatibility with original Minecraft Beta 1.7.3 worlds and servers is a **second, much later priority**. Keep core game data and rendering reasonably separate so adapters can be added later, but do not delay gameplay work or shape every internal API around the old file format, block IDs, or network protocol. When compatibility work begins, put format and protocol translation at explicit boundaries.

# Reference sources

- The decompiled Beta 1.7.3 Java source is checked in under `refs/mc_b1.7.3_release/1.7.3-LTS/src/minecraft/net/minecraft/{src,client}/` (client) and `.../minecraft_server/net/minecraft/src/` (server). Treat it as the behavioral source of truth for generation, block/item behavior, recipes, and options rather than guessing from memory. The Java source files are large; do not read them in their entirety. Use `grep`/`sed`/`rg` to find and inspect only the relevant methods and fields.
- `refs/mc_b1.7.3_release` is a nested git repo recorded as a gitlink with no `.gitmodules`, so it may be missing in a fresh clone; `refs/` is otherwise not ignored.
- `docs/PLAN.md` is an aspirational layout sketch; this file and the code are authoritative.
- The feature/settings workflow lives in `.grok/skills/implement-feature/SKILL.md` (when a `GameSettings` toggle is warranted, menu wiring, when to remesh, and tests). Read it before adding a graphics option.

# Commands

- `cargo run` — start the game. `cargo run --features dev_dynamic_linking` links faster while iterating.
- `cargo nextest run` — all tests. Focus one target with `cargo nextest run --test <name>` (for example `--test world`) or one test with `cargo nextest run <filter>`.
- `cargo check` / `cargo fmt` for a quick pass. `cargo fmt` must run on nightly: `rust-toolchain.toml` pins nightly and `rustfmt.toml` enables unstable `imports_granularity = "Item"`.
- Edition 2024 is in use (let-chains appear in the code, e.g. `if let ... && let ...`).

# Architecture

Organize code by gameplay and engine subsystem, not broad `components/` and `systems/` directories. Add modules as their behavior is implemented; the planned layout below is a guide, not a request to create empty files.

- `src/app/`: application assembly, game states, and loading. The top-level plugin should compose subsystem plugins.
- `src/block/`: compact block values, definitions, properties, materials, and registry.
- `src/world/chunk/`: chunk coordinates, compact block storage, management, and lifecycle.
- `src/world/generation/`: staged terrain generation, including noise, biomes, caves, ores, and structures.
- `src/world/meshing/`: visible faces, mesh construction, lighting data for meshes, and background mesh jobs.
- `src/world/streaming/`: chunk loading, unloading, view distance, and work priorities.
- `src/world/textures/`: the terrain atlas plugin, block-face tile mappings, and climate-based grass colors.
- `src/world/lighting/`: sunlight, block light, and propagation in world data.
- `src/world/tick.rs`: the shared 20 Hz `WorldTick` clock. Tick-counted simulation reads this instead of keeping a private accumulator.
- `src/world/persistence/`: saving and loading. Add Beta format adapters here when compatibility becomes a priority.
- `src/player/`: controller, movement, camera, interaction, mining, and placement.
- `src/physics/`: voxel collision, raycasting, and gravity.
- `src/entity/`: non-block entities, health, spawning, mobs, and dropped items. Shared body components (`EntitySize`, `Velocity`, `Gravity`, `CollisionState`, `StepHeight`) live here. Dropped items and block particles are implemented; mobs are not yet.
- `src/item/` and `src/inventory/`: item definitions, stacks, tools, slots, hotbar, and inventory transfer.
- `src/crafting/`: crafting grids, shaped and shapeless recipes, and the Beta 1.7.3 recipe book.
- `src/gameplay/`: weather, damage, respawning, and other game rules. Day time is `WorldTick::world_time` in `src/world/tick.rs`, not a second clock.
- `src/rendering/`: materials, textures, shaders, fog, and sky.
- `src/ui/`, `src/input/`, and `src/audio/`: presentation, controls, and sound. `src/ui/` currently holds the menu, settings screen, HUD, inventory GUI, block icons, and stack overlays, composited by a dedicated UI camera.
- `src/networking/`: multiplayer and, eventually, Beta 1.7.3 protocol adapters. Do not introduce protocol constraints into the core simulation prematurely.
- `src/util/`: small shared utilities that do not belong to a specific subsystem.
- `assets/`: local Minecraft Beta reference content. It is currently checked in (the introducing commit says it will be removed before release), but it is not distributable game content; players will supply their own texture ZIP later. Do not add more Minecraft files here.
- `tests/`: all tests for this repository, including tests for individual modules and integration behavior. Do not put test modules in `src/`.
- `benches/`: targeted performance benchmarks.

The current game has a walking, sprinting, sneaking, and jumping player with voxel collision, step height, view bobbing, and a first-person arm; full-height Beta-style generated chunks; skylight and torch block light; chunk meshes with streaming around the player; and terrain atlas rendering with climate-sampled grass and foliage colors. Block interaction raycasts into world data, breaks blocks with per-block stages and tool speed and durability, and places blocks including torches. Items, a nine-slot hotbar, a 27-slot main inventory, a 2×2 crafting grid, a workbench session, dropped item entities, block break and hit particles, a HUD, menus, and settings are implemented. A custom save format under `saves/` persists the world, player, inventory, dropped items, and the world's `world_time`. A shared 20 Hz `WorldTick` advances while a world is being played. `src/main.rs` is the executable entry point, and `src/lib.rs` exposes modules for reuse and tests. The remaining directories describe future responsibilities; add them only as working features require them.

# Implemented conventions

- Item IDs: block items reuse their Beta block ID, and standalone items start at 256. Resolve raw IDs through the item registry before use.
- `ItemStack` is a validated, nonempty stack. Its private fields enforce registry stack limits and per-item data rules, so an empty slot is `None`, never a zero-count stack.
- Keep Beta behavior in simulation data (block states, item data, the recipe book) and keep presentation in `src/world/textures/` and `src/ui/`. Do not fold rendering concerns into world or item data.
- Persistence uses a custom versioned format, not the Beta region format. Keep it behind `src/world/persistence/` so a Beta adapter can be added at that boundary later.
- Client options live in `settings.json` and are owned by `src/app/settings.rs`; gameplay reads them through `GameSettings` rather than reading the file directly.
- Textures use nearest sampling and no mipmaps. `main` sets that on `ImagePlugin`, and loaders should leave `ImageSampler::Default` so they inherit it. An image sets its own sampler only for a different address mode, such as repeating clouds. The terrain atlas also pins `lod_max_clamp` to 0 so a mip cannot blend neighboring tiles.

## World tick

`WorldTick` in `src/world/tick.rs` is the only 20 Hz clock, matching Beta's `Timer`. `WorldPlugin` advances it in `First` from virtual time while `AppScreen::Playing`. The menu does not accumulate ticks, so returning to the game does not replay a catch-up burst.

- 20 ticks per second. A frame's delta is clamped to 1 second, and at most 10 ticks are emitted. Whole ticks beyond that are discarded. `partial()` is the leftover fraction in `0..1`, used to interpolate renders between the last tick and the next.
- Step simulation with `ticks_this_frame()`. Sample `just_pressed` once per frame, then run held-button work inside the tick loop. Bevy keeps `just_pressed` true for the whole frame, so reading it inside the loop would repeat a click on every catch-up tick.
- Keep this on `WorldTick` in `First`. A `FixedUpdate` schedule would repeat `just_pressed` the same way, and pausing Bevy's fixed clock would also freeze frame-time systems.
- `world_time` counts ticks since the world started. `DAY_LENGTH` is 24000. The sky reads it, with `partial()`, for the sun and moon. It is stored on `WorldManifest` with `#[serde(default)]`, so an older `level.json` loads at time 0, and autosave writes it back.
- Consumers today: block breaking and the place repeat, arm swing and equip, dropped items and the hotbar pop, block particles, and the water atlas. Leaf wiggle and view bob stay on frame time.
- New tick-driven work (block entities, scheduled block updates, weather) consumes this clock. Do not add another 20 Hz accumulator.
- All block tick updates (random ticks, scheduled block updates) are deferred for now. No block performs per-tick simulation.

# Data and performance rules

- Store blocks as compact values inside chunks. Do not represent every world block as a Bevy entity. Use Bevy entities for rendered chunks and for independently simulated objects such as players, mobs, and dropped items.
- Keep the world data path clear: generation produces chunk blocks, heightmaps, and per-column biome climate; lighting derives skylight; meshing reads blocks, light, and climate to build atlas UVs and grass vertex colors; Bevy renders the resulting chunk meshes.
- Use generated temperature and humidity to sample `grasscolor.png` for grass tops. The discrete biome name alone is not enough for this color. Keep visual coloring in textures and meshing, separate from the local C++ terrain-generation reference.
- Player interaction should raycast into world data, modify the relevant chunk, mark affected chunks dirty, and schedule remeshing. Handle chunk boundaries when a changed block affects neighboring meshes or light.
- Keep generation, lighting, meshing, and streaming distinct. Run expensive independent work off the main thread where practical, then apply results to Bevy assets and entities on the appropriate thread. Prioritize nearby or visible chunks.
- Prefer data-oriented storage, bounded allocations, and reusable buffers in hot paths. Profile before adding complex optimizations; use benchmarks for generation, meshing, and streaming changes with meaningful performance risk.
- Keep deterministic game rules and world representation separate from client rendering where practical. Consider a separate voxel crate or workspace only when the growing codebase or server work gives a concrete reason for that split.
- Chunks store raw block bytes (`Chunk::raw_blocks`); `Id` itself is two bytes because of its `Unknown(u8)` catch-all.
- Block meshes use one packed 16-byte vertex (`meshing/vertex.rs`, decoded by `textures/block_vertex.wgsl`) and 16-bit indices when they fit. Vertices carry raw sky and block light samples, AO levels, and tints; the `BlockMaterial` uniform applies `skylight_subtracted`, old lighting, and smooth lighting. Time of day and lighting settings must not rebuild meshes; only geometry changes (blocks, fancy leaves) do. Keep `BlockVertex::color` in step with the shader.
- Chunks render as 16×16×16 sections. Streaming keeps a light fingerprint per section, so an edit (`WorldStreaming::request_block_update`) relights the chunks its light can reach and rebuilds only sections whose blocks or light changed.
- Avoid mutating assets every frame. Animate with Bevy's `globals.time` in shaders, pass per-entity values through `MeshTag` (see `TintedMaterial`), move entities instead of UVs, and write animated atlas tiles from the render world (`textures/water.rs`) rather than modifying the atlas `Image`.
- Sun shadow maps are only enabled when terrain is lit (`GameSettings::sun_shadows`); under old lighting every material is unlit and nothing samples them.

# Working in this repo

- Implement as much of the requested behavior as is practical in each task. Prefer complete, usable features over scaffolding, speculative abstractions, or APIs with no working implementation. Add a module, trait, plugin, or configuration option when it serves an actual feature or a clear near-term need.
- Follow the existing Rust style and use Bevy APIs supported by the version in `Cargo.toml`.
- Put all tests under the repository root's `tests/` directory. Add tests for behavior and invariants that matter, especially chunk indexing, coordinates, generation determinism, lighting, and block interaction. A standalone Bevy mesh test may use one entity for a block; that does not set the production world representation.
- Keep changes focused and avoid filling planned modules with placeholders. Run `cargo fmt` and relevant checks or tests for code changes, and report any verification limits.
- Preserve the user's in-progress changes. The existing source files and tests may be mid-implementation.
- Treat `assets/` as local, reference-only Minecraft Beta content (`terrain.png`, `misc/grasscolor.png`, `misc/foliagecolor.png`, the `gui/` textures, `font/minecraft.otf`). Do not add more files from the original game, and keep the game able to start when these reference files are absent. Prefer not to touch `assets/` in commits unless the task requires it.
- Tests are split between flat files (`tests/<name>.rs`) and multi-file targets rooted at `tests/<name>/main.rs` (for example `tests/world/main.rs`, target `world`) that mirror the matching `src/<name>/` tree. Engine tests build a headless `App` with `MinimalPlugins` + `AssetPlugin` (and `MeshPlugin`), not `DefaultPlugins`, so they run without a GPU or window.
- `settings.json`, `saves/`, and `screenshots/` are gitignored runtime output. In-game F2 writes to `screenshots/`.
