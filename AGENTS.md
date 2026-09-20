# Project direction

Build a Minecraft Beta-inspired game in Rust with Bevy. The first priority is a playable, performant recreation of the Beta experience: terrain, blocks, movement, interaction, lighting, inventory, mobs, and the other systems needed for the game to feel right. Prefer idiomatic Rust and Bevy implementations, and measure performance in the paths that matter most.

Compatibility with original Minecraft Beta 1.7.3 worlds and servers is a **second, much later priority**. Keep core game data and rendering reasonably separate so adapters can be added later, but do not delay gameplay work or shape every internal API around the old file format, block IDs, or network protocol. When compatibility work begins, put format and protocol translation at explicit boundaries.

# Architecture

Organize code by gameplay and engine subsystem, not broad `components/` and `systems/` directories. Add modules as their behavior is implemented; the planned layout below is a guide, not a request to create empty files.

- `src/app/`: application assembly, game states, and loading. The top-level plugin should compose subsystem plugins.
- `src/world/block/`: compact block values, definitions, properties, materials, and registry.
- `src/world/chunk/`: chunk coordinates, compact block storage, management, and lifecycle.
- `src/world/generation/`: staged terrain generation, including noise, biomes, caves, ores, and structures.
- `src/world/meshing/`: visible faces, mesh construction, lighting data for meshes, and background mesh jobs.
- `src/world/streaming/`: chunk loading, unloading, view distance, and work priorities.
- `src/world/lighting/`: sunlight, block light, and propagation in world data.
- `src/world/persistence/`: saving and loading. Add Beta format adapters here when compatibility becomes a priority.
- `src/player/`: controller, movement, camera, interaction, mining, and placement.
- `src/physics/`: voxel collision, raycasting, and gravity.
- `src/entity/`: non-block entities, health, spawning, mobs, and dropped items.
- `src/item/` and `src/inventory/`: item definitions, stacks, tools, slots, hotbar, and crafting.
- `src/gameplay/`: time, weather, damage, respawning, and other game rules.
- `src/rendering/`: materials, textures, shaders, fog, and sky.
- `src/ui/`, `src/input/`, and `src/audio/`: presentation, controls, and sound.
- `src/networking/`: multiplayer and, eventually, Beta 1.7.3 protocol adapters. Do not introduce protocol constraints into the core simulation prematurely.
- `src/util/`: small shared utilities that do not belong to a specific subsystem.
- `assets/`: game assets; group textures by blocks, items, entities, and UI as needed, with separate models, shaders, audio, fonts, and data when those appear.
- `tests/` and `benches/`: behavior tests and targeted performance benchmarks.

The current repository only has the beginnings of `app`, `world/block`, `world/chunk`, `world/generation`, and `player`. `src/main.rs` is the executable entry point, and `src/lib.rs` exposes modules for reuse and tests. Extend these areas incrementally rather than treating the proposed layout as already implemented.

# Data and performance rules

- Store blocks as compact values inside chunks. Do not represent every world block as a Bevy entity. Use Bevy entities for rendered chunks and for independently simulated objects such as players, mobs, and dropped items.
- Keep the world data path clear: generation writes chunk storage; lighting updates world light data; meshing reads chunk and light data; Bevy renders the resulting chunk meshes.
- Player interaction should raycast into world data, modify the relevant chunk, mark affected chunks dirty, and schedule remeshing. Handle chunk boundaries when a changed block affects neighboring meshes or light.
- Keep generation, lighting, meshing, and streaming distinct. Run expensive independent work off the main thread where practical, then apply results to Bevy assets and entities on the appropriate thread. Prioritize nearby or visible chunks.
- Prefer data-oriented storage, bounded allocations, and reusable buffers in hot paths. Profile before adding complex optimizations; use benchmarks for generation, meshing, and streaming changes with meaningful performance risk.
- Keep deterministic game rules and world representation separate from client rendering where practical. Consider a separate voxel crate or workspace only when the growing codebase or server work gives a concrete reason for that split.

# Working in this repo

- Implement as much of the requested behavior as is practical in each task. Prefer complete, usable features over scaffolding, speculative abstractions, or APIs with no working implementation. Add a module, trait, plugin, or configuration option when it serves an actual feature or a clear near-term need.
- Follow the existing Rust style and use Bevy APIs supported by the version in `Cargo.toml`.
- Add tests for behavior and invariants that matter, especially chunk indexing, coordinates, generation determinism, lighting, and block interaction. A standalone Bevy mesh test may use one entity for a block; that does not set the production world representation.
- Keep changes focused and avoid filling planned modules with placeholders. Run `cargo fmt` and relevant checks or tests for code changes, and report any verification limits.
- Preserve the user's in-progress changes. The existing source files and tests may be mid-implementation.
- Treat `assets/` as local, reference-only Minecraft Beta content. Never commit files from it. Players will eventually supply their own texture ZIP; the current `terrain.png` loader is a development path, not a bundled asset.
