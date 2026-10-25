# Performance

The rules these measurements led to are in [AGENTS.md](../AGENTS.md), under
"Data and performance".

## Idle checks

`Max FPS` in Settings defaults to 60. It cycles through 90, 120, 144, 240,
VSync (no additional application cap), 30, and back to 60. Existing settings
files inherit 60 when the field is absent. Menus use a 30 Hz timer and respond
to window input; unfocused gameplay uses 20 Hz, and unfocused menus use 5 Hz.
The world simulation still uses the shared 20 Hz `WorldTick`.

A windowed game on Linux paces itself to 99.5% of its display's refresh rate
instead of leaving it to VSync, whatever `Max FPS` allows above that. Measured
on KDE Wayland with an NVIDIA card at 144 Hz, a composited window blocked on
VSync ran its frames in pairs 1 ms and 13 ms apart; fullscreen, or a timer just
under the refresh rate, gave an even 7 ms.

The ten-second console performance report includes `streaming scans`. Once
terrain generation, population, and first meshes settle, candidate-discovery
passes should fall to zero unless movement, settings, job completion, or a
chunk membership change requires more work. Edits can still relight and remesh
chunks. Random block ticks and animated fluids continue while playing.

To compare power and performance, use the same saved world, graphics quality,
render distance, window size, and camera direction. Wait for loading to finish.
Compare standing still with inventory closed and open, then compare 60 FPS
with VSync. Also check menus and an unfocused window. Use a release build for
runtime profiling (`cargo run --release`); development game code is
unoptimized. Record CPU utilization, actual FPS, CPU/GPU frame time,
and power if the available profiler supports it. A frame cap alone changes
how often work runs, so compare per-frame costs at equal FPS as well.

Regression tests cover unchanged UI components, zero pickup animation
counters, settled streaming and its wakeups, cloud material updates, camera
projection changes, and pacing settings. They verify avoided work and gameplay
behavior; they do not establish a measured power or GPU bandwidth reduction.

## Render-side numbers

Chunk section layers are quad records in one storage buffer
(`rendering/chunk_quads.rs`). The report's `mesh memory` line is the bytes
layers hold in it and `quad buffer` is its reserved size; the buffer grows by
a quarter (at least 16 MiB) at a time, so the gap stays small. `mesh slabs`
now only covers the shared proxy meshes, entity models, and dropped blocks.

`MC_CHUNK_QUADS=off` stores layers as packed vertex meshes instead, as they
were before. Measured with `examples/chunk_render.rs` on an Apple M5 Pro
(Metal), dev profile, render distance 8, Fancy, 289 chunks and about 2,780
section layers of seed-0 terrain:

- Vertex meshes: 73.3 MiB of layer geometry (76 bytes per quad).
- Quad records: 31.0 MiB (32 bytes per quad, plus one header per layer and a
  second record for each sloped fluid top), 58% less.
- Screenshots of the two differ in 0.03% to 0.2% of pixels under old
  lighting, all on crossed plants, whose 0.05-block inset is not on the
  1/64-block grid; 0.8% with new lighting and sun shadows, in the same places.
- Frame time was not separable: macOS paced most runs to the display.

At startup the log states the backend, whether `MULTI_DRAW_INDIRECT_COUNT` is
available, and the chunk culling mode chosen from it. Bevy also logs whether
GPU preprocessing is fully supported; if it is not, chunks are not on the
indirect path at all.

`MC_CHUNK_CULLING=gpu` or `cpu` forces how section layers are frustum culled.
`gpu` skips Bevy's main-world test for the camera and each shadow cascade and
leaves culling to its GPU pass. It is the default only where the driver can
skip culled draws (`MULTI_DRAW_INDIRECT_COUNT`).

Measured on an Apple M5 Pro (Metal), dev profile, render distance 12, Fancy,
old lighting, one open-terrain view with 5,864 section layers:

- CPU and GPU culling both held the display's 120 FPS, so this view cannot
  rank them.
- Bevy's `OcclusionCulling` with a `DepthPrepass` on the player camera dropped
  to roughly 85 to 100 FPS. The prepass draws visible geometry a second time at
  4x MSAA and cost more than the occluded sections saved, so it was not kept.
  A cave view was not measured.
- Bevy's `RenderDiagnosticsPlugin` pass timings read about 0.02 ms per pass
  here and did not reflect draw cost, so they are not in the report.

### Chunk regions, cave culling, and facing culling

These follow Sodium's renderer, reimplemented from its ideas only (Sodium is
under the PolyForm Shield license, so none of its code is used).

- **Cave culling** (`rendering/occlusion.rs`): each mesh job flood-fills its
  sections and records which faces see each other. When the camera enters
  another section, a walk outward from it finds the sections reachable
  through open space. A camera inside an opaque cube turns it off.
- **Facing culling**: a layer's records are grouped by facing, and a region's
  draw list leaves out the groups that face away from the camera.
- **Chunk regions** (`rendering/chunk_regions.rs`,
  `world/streaming/regions.rs`): one entity per layer for each 8×8 chunk
  columns, drawing a list of record runs kept in the quad buffer. The vertex
  shader binary-searches the list. Lists are rebuilt when the camera's
  clip-from-world matrix or a section layer changes.
- **Water** is sorted back to front per section, and regions list water
  sections farthest first.

Measured with `examples/chunk_render.rs` on an Apple M5 Pro (Metal), seed
0, Fancy, 1920x1080, `MSAA=off`. "Old" is `MC_CHUNK_REGIONS=off
MC_OCCLUSION=off`.

| Distance, view               | Old draws | Region draws | Sections reached | Quads drawn | List rebuild (`fast`) |
| ---------------------------- | --------- | ------------ | ---------------- | ----------- | --------------------- |
| 8, `VIEW=0` (dev profile)    | ~2,780    | 18           | 1,278 of 2,312   | 130,814     |                       |
| 16, `VIEW=0`                 | ~8,350    | 21           | 5,388            | ~178,000    | 0.26 ms               |
| 24, `VIEW=0`                 | ~17,730   | 25           | 12,176           | ~201,000    | 0.70 ms (2.0 longest) |
| 16 or 24, `VIEW=3` (a cave)  | ~8,350–17,800 | 1        | 12               | 1,217       | 0.01–0.03 ms          |

- Screenshots against the old path differ in at most 0.024% of pixels,
  the same as two runs of the old path differ from each other. The cave and
  canyon views match exactly.
- Frame time was not separable. Every run of both paths was paced to the
  display (11.1 ms at 90 Hz). Unpaced runs, probably with the window covered,
  showed about 1.9 ms at distance 24 with regions, but no old-path run went
  unpaced to compare against.
- The draw-list rebuild is main-thread work on frames where the camera moved.
  Water sorting is capped at 64 sections a frame.
- Before these changes, `VIEW=1` already sat inside stone, an x-ray view. It
  is kept, and shows why culling is off inside an opaque cube.

## Memory

The report's `process memory` line is resident memory from Bevy's
`SystemInformationDiagnosticsPlugin`. On macOS that leaves out most GPU
allocations, which are the larger share. `footprint -p game` shows the whole
figure (what Activity Monitor calls Memory) split by kind: `graphics` rows
are GPU textures and buffers, `Malloc` rows are the heap.

`examples/chunk_render.rs` takes `DISTANCE`, `SIZE`, `MSAA=off`, and `HOLD`
for this: `HOLD=30` keeps the settled view open long enough to read it.

Measured on an Apple M5 Pro (Metal), dev profile, render distance 4 (81
chunks, 828 section layers, 9.6 MiB of quad records), old lighting, a
2560x1440 window, `footprint`'s `phys_footprint`:

| Build                                         | Total  | GPU    | Heap   |
| --------------------------------------------- | ------ | ------ | ------ |
| Bevy defaults, 4x MSAA                        | 694 MB | 472 MB | 174 MB |
| Small unused shadow maps                      | 672 MB | 435 MB | 172 MB |
| and only the Bevy features in use             | 613 MB | 433 MB | 134 MB |
| and Anti-aliasing off                         | 487 MB | 331 MB | 113 MB |

The GPU column repeats within a few MB. The heap column does not: its
`Malloc Large` part was anywhere from 22 to 73 MB between otherwise equal
runs, so read heap differences under about 50 MB as noise. The world itself is a small part of any row: the
chunk blocks, light, and quad records together are under 30 MB.

- Bevy creates a point light cube array (24 MiB at its default size) and a
  directional array (16 MiB per cascade) even with no shadow caster.
  `rendering/plugin.rs` keeps both at 16 pixels; the game spawns neither light.
  PBR remains for mesh batching, instancing, and drawing; its lighting
  dependency and fallback bindings cannot be removed independently.
- Beta lighting is permanent. Every 3D camera disables light clustering, and
  GPU light clustering is explicitly disabled independently of mesh GPU
  preprocessing/culling: Bevy 0.19.1 otherwise creates a zero-sized clustering
  dummy texture for `ClusterConfig::None`. Block, creature, and tinted
  materials disable shadow/prepass participation.
  The game and render harness disable the deferred lighting plugin. Fast and
  Fancy retain transparent Beta water; Ultra/SSR and their HDR camera path
  have been removed. Smooth lighting still updates a uniform without remeshing.
- Tonemapping LUTs and KTX2/Zstandard support are disabled; all window cameras
  use `Tonemapping::None`, and the game loads PNG textures. Legacy Ultra
  settings load as Fancy; obsolete lighting and ambient-brightness fields
  are ignored and omitted on the next settings save.
- 4x MSAA is a multisampled colour and depth target at window size, about
  105 MB here and proportional to the window's pixels. `Msaa` must match on
  every camera, UI included, or the targets stay allocated.
- Bevy's default features start plugins the game never uses. Dropping them
  shrank the binary by 17%; the 40 to 60 MB the total fell by is mostly heap
  and within that noise.
- wgpu validates indirect draws in dev builds, about 17 MB of buffers that a
  release build does not create (`WGPU_VALIDATION_INDIRECT_CALL=0` removes
  them in dev). Its other debug flags made no measurable difference.
- `MemoryHints::MemoryUsage` in `main.rs` only affects wgpu's Vulkan and DX12
  allocators. It is not measured; Metal has no such allocator.

What is left is mostly a floor the game does not control. A Bevy 0.19 app
with the default plugins, one camera, and one cube in a 960x540 window is
already about 380 MB in a dev build. 176 MB of that is 44 GPU allocations of
exactly 4 MiB (22 before any camera exists). Their count did not change with
the UI, anti-aliasing, or post-processing plugins, pipelined rendering, the
wgpu debug flags, the window size, or the number of cameras, and their owner
was not identified.

Each additional `Camera3d` added about 30 MB in that test (per-view buffers
and heap). The game now runs two on the window while playing (world and arm)
plus the UI camera; see [Camera stack](#camera-stack).

A release build of the last two rows measured 586 MB with 4x MSAA (GPU 354
MB) and 527 MB without (GPU 316 MB), so the dev profile is not what makes
the footprint large, and the MSAA saving was smaller there, about 40 MB.

### Camera stack

Every camera on the window draws into one shared main texture, and each of
its passes loads and stores that whole texture (four samples a pixel with
anti-aliasing) and its own depth texture. Bevy begins a camera's opaque pass
even when it has nothing to draw. By default each camera then also copied
the main texture to the swapchain, the later ones alpha-blending over the
earlier.

The game used to run five cameras while playing: sky (which drew nothing and
only cleared to the sky color), celestial (sun, moon, stars, sunrise fan),
world, arm, and UI. Now:

- The world camera clears to the sky color and draws the sky bodies itself
  with `SkyMaterial` (`rendering/textures/sky_backdrop.rs`). Its vertex
  shader puts them on the far plane, so terrain covers them at any distance,
  and its `depth_bias` sorts them ahead of every other blended mesh.
- `composite_window_cameras` (`rendering/plugin.rs`) lets only the
  highest-order active window camera copy the frame to the swapchain, as a
  plain replace. The others skip their copy.
- Anti-aliasing is off by default, as in Beta.

The arm camera remains: it needs its own depth. Merging it, and then drawing
the HUD through the world camera, would leave one camera.

#### Measuring frame time on macOS

A visible window is paced by the compositor. On the Apple M5 Pro (120 Hz
display) `examples/chunk_render.rs` averaged 12.2 ms a frame, alternating
between refresh intervals, whatever it drew: a 320x180 window at render
distance 2 took as long as 3024x1890 at 16, with or without anti-aliasing.
Bevy's default `WinitSettings` also drops an unfocused window to 60 Hz; the
harness now updates continuously. `HIDDEN=1` opens the window invisible,
which removes the pacing; frame times then follow resolution and
anti-aliasing, so the GPU work is still done, but the screenshot is black.
Take screenshots from a visible window and timings from a hidden one.

Medians of three interleaved runs of 600 frames each, `--profile fast`, seed 0,
3024x1890, `HIDDEN=1`, anti-aliasing off unless noted. `TURN=1` turns the
camera every frame, so the draw lists are filtered every frame. "Before" is
commit `8b1d6d5` with the same harness.

| View                          | Before   | After   | Draw lists before (avg / max) | After (avg / max) |
| ----------------------------- | -------- | ------- | ----------------------------- | ----------------- |
| `VIEW=0`, distance 16, 4x MSAA | 2.16 ms | 1.36 ms |                               |                   |
| `VIEW=0`, distance 16          | 1.83 ms | 1.37 ms |                               |                   |
| `VIEW=2`, distance 16          | 1.76 ms | 1.41 ms |                               |                   |
| `VIEW=3` (a cave), distance 16 | 1.75 ms | 1.33 ms |                               |                   |
| `VIEW=0`, distance 16, turning | 2.48 ms | 1.51 ms | 0.78 / 2.28 ms                | 0.05 / 0.09 ms    |
| `VIEW=0`, distance 24, turning | 3.65 ms | 1.54 ms | 1.85 / 5.63 ms                | 0.07 / 0.14 ms    |

- The after column sits at the harness's floor: a 320x180 window at distance
  2 took 1.4 to 1.5 ms before these changes. Anti-aliasing no longer changes
  its frame time at this size, so what is left is CPU work.
- The harness has no arm or UI camera, so it shows three cameras becoming
  one. The game goes from five to three, and from five swapchain copies to
  one.
- Draw lists used to be rebuilt from scratch whenever the view matrix
  changed: every reached section sorted by distance, and two hash lookups
  per section. Now the order is kept until a layer, cave culling, or the
  camera's block changes, and turning only walks it. A column wholly outside
  the frustum skips all its layers with one test. The longest rebuild left
  is the reorder itself, about 1.2 ms at distance 24, when the camera enters
  another block.
- Screenshots of the two builds at distance 12 (1280x720, visible window)
  matched exactly for views 2 and 4x MSAA view 0, rain over view 2, and the
  sky at noon, at dusk to the west, and at midnight to the east and west
  (`VIEW=5`/`6`, `TIME`; both look up through tree leaves). Views 1 and 4 and the
  Nether differed in 0.015% to 0.9% of pixels, and view 0 without MSAA in
  1.7%, where a tree canopy was present in one run and not the other; the
  two builds draw the same lists for the same sections (a test checks the
  column culling against testing each layer).
- The harness sometimes captures a blank frame, in either build: rerunning
  the same view gave the old build's image once and a blank one once.

### Permanent Beta lighting cleanup

Paired dev-profile runs on Metal at 960x540, MSAA off, seed 0, smooth lighting
on, leaf wiggle off, measured after settling with `footprint`. Both builds used
`Tonemapping::None` and the trimmed texture feature list to isolate renderer
changes. The baseline retained the original clustering and deferred plugin;
the optimized build disabled GPU light clustering, all per-camera clusters,
deferred lighting, and custom-material shadow/prepass participation.

| View / distance | Baseline total / GPU | Optimized total / GPU | Baseline / optimized frame time |
| --- | --- | --- | --- |
| View 0 / 4 (81 chunks) | 420 / 246 MB | 386 / 223 MB | 5.63 / 5.42 ms |
| View 1 / 8 (289 chunks) | 484 / 274 MB | 441 / 241 MB | 5.70 / 5.44 ms |

GPU is the sum of the dirty graphics rows, rounded to MB. These runs saved
23–33 MB of GPU memory. Total footprint saved 34–43 MB, but the heap portion
is noisy; these figures are observations, not a guaranteed budget. macOS
presentation pacing also limits the frame-time comparison; neither scene
showed a regression. The distance-8 screenshots matched exactly; distance 4
had one differing pixel out of 518,400. A handful of off-screen section layers
varied between runs without affecting either captured view.

The packed-vertex and quad-record paths were also rendered with smooth lighting
both on and off. Their screenshots differed by 0.029% and 0.028% respectively,
consistent with the existing quantized geometry path. A separate GPU smoke
scene exercised creature skin/hurt/flash, glow, charge, instance tint, and
multiply blending without shader or render validation errors. The normal
application also started without rendering errors; this did not include a
full interactive gameplay session.

### Movement caches (October 14, 2026)

Moving across a block boundary used to recreate every reached layer's
metadata, column bounds, and region slots before sorting it. Those only change
when layers arrive or cave visibility changes. Movement now recalculates
distances in the retained layer arrays and sorts those; turning still only
filters them. Layer revisions and cave visibility invalidate the metadata.
Opaque layers remain nearest first and water remains farthest first.

Cave traversal now uses retained dense arrays for section connectivity,
visited faces, its queue, and column masks. It publishes one hash-map entry
per reached column instead of hashing for every visited section. Missing
columns inside the graph bounds remain open.

Measured on an Apple M5 Pro, Metal, **release** (fat LTO), Bevy 0.20,
960×540, Fancy, MSAA off, `VIEW=0`, hidden window, no screenshots, and 1,800
timed frames after settling. The baseline is `8723961` with the same added
harness controls. Distance 24 movement is the median of three interleaved
before/after pairs; the other rows are single pairs.

| Case | Before frame time | After frame time | Before draw lists | After draw lists |
| --- | --- | --- | --- | --- |
| Distance 24, moving | 1.80 ms | 1.55 ms | 0.427 ms | 0.189 ms |
| Distance 16, moving | 1.32 ms | 1.25 ms | 0.178 ms | 0.088 ms |
| Distance 24, stationary | 1.07 ms | 1.07 ms | no rebuilds | no rebuilds |

Distance 24 moving frame times were 1.69/1.80/1.80 ms before and
1.44/1.55/1.56 ms after: about 14% less frame time at the medians. The moving
view reached 12,189 sections and drew 30 regions in every run. Quads drawn
varied by about 1% across runs because terrain population is not identical
between runs. The longest draw-list rebuild still occurs when the visible
set changes: 0.984–1.001 ms before and 0.897–0.916 ms after.

An independent all-open, distance-24 CPU traversal benchmark (19,208 sections,
20 warmup walks and 200 measured walks, `fast` profile) fell from
0.751–0.757 ms median to 0.446–0.458 ms. This synthetic case measures
traversal cost, not whole-game FPS.

Reproduce the movement measurement from the checkout:

```sh
CARGO_TARGET_DIR="$PWD/target" BEVY_ASSET_ROOT="$PWD" \
HIDDEN=1 NO_SCREENSHOT=1 MSAA=off \
DISTANCE=24 MOVE=1 FRAMES=1800 \
cargo run --release --features render_harness --example chunk_render
```

Omit `MOVE` for the stationary check; `TURN=1` continues to test turning.
`MOVE` follows a closed path across section boundaries while the player stays
at the loading origin, keeping streaming settled. `NO_SCREENSHOT` skips the
capture request entirely. The CPU benchmark runs with
`cargo test --profile fast --features render_harness -- --ignored benchmark_visibility_walk --nocapture`.

Regression tests cover section-centre reconstruction across positive and
negative regions, walls changing between walks, missing columns, and graph
bounds growing, shrinking, and emptying. The GPU runs completed without
render validation errors. No screenshots were taken. These results cover
the terrain harness, which has no arm, HUD, or populated mobs; a full gameplay
session can have other bottlenecks.

## Build times

Measured on an Apple M5 Pro (18 cores) with dependencies already built,
October 2026.

- `cargo test` used to build 31 test binaries plus two empty unit-test
  harnesses, each statically linking Bevy. It is now one binary: 794 tests
  build and run in about 8 s warm. A name filter goes after `--`: given
  before it (`cargo test name`), Cargo builds the lib's empty harness again,
  `test = false` or not.
- Development builds compile game code at `opt-level = 0` with full debug info
  (dependencies stay at 3 with line tables only). A one-line edit rebuilds in
  about 2.6 s, the game crate from scratch in about 12 s, and a cold build with
  the test run takes about 3.5 minutes. At `opt-level = 1` the same edits took
  3 to 11 s.
- The cost is runtime: the test suite runs in about 27 s instead of 3.5 s, and
  generation and meshing are slower in a dev build. Judge frame times and
  stutter in `cargo run --release` only.
- Release uses fat LTO in one codegen unit; a cold `cargo build --release`
  takes about 6 minutes.
- `--profile fast` is release with `lto = "off"`, 16 codegen units and
  incremental compilation. On a 16-core Linux machine a cold
  `cargo build --profile fast --bin game` took 1m 31s and a one-line edit
  rebuilt in about 10 s, against 5m 50s for the same rebuild on `--release`.
  Its runtime cost has not been measured.
- `dev_dynamic_linking` (11.4 s against 10.8 s for the same edit at `opt-level = 1`) and
  `-Zthreads=8` did not help, so neither is configured.
- Changing `RUSTFLAGS`, features, or profile settings rebuilds Bevy (1.5 to 2
  minutes here, far longer cold). Keep them the same across `run` and `test`.
