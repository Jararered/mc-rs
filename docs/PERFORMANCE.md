# Idle performance checks

`Max FPS` in Settings defaults to 60. It cycles through 90, 120, 144, 240,
VSync (no additional application cap), 30, and back to 60. Existing settings
files inherit 60 when the field is absent. Menus use a 30 Hz timer and respond
to window input; unfocused gameplay uses 20 Hz, and unfocused menus use 5 Hz.
The world simulation still uses the shared 20 Hz `WorldTick`.

The ten-second console performance report includes `streaming scans`. Once
terrain generation, population, and first meshes settle, candidate-discovery
passes should fall to zero unless movement, settings, job completion, or a
chunk membership change requires more work. Edits can still relight and remesh
chunks. Random block ticks and animated fluids continue while playing.

To compare power and performance, use the same saved world, graphics quality,
render distance, window size, and camera direction. Wait for loading to finish.
Compare standing still with inventory closed and open, then compare 60 FPS
with VSync. Also check menus and an unfocused window. Use a release build for
runtime profiling (`cargo run --release`); development game code uses a lower
optimization level. Record CPU utilization, actual FPS, CPU/GPU frame time,
and power if the available profiler supports it. A frame cap alone changes
how often work runs, so compare per-frame costs at equal FPS as well.

Regression tests cover unchanged UI components, zero pickup animation
counters, settled streaming and its wakeups, cloud material updates, camera
projection changes, and pacing settings. They verify avoided work and gameplay
behavior; they do not establish a measured power or GPU bandwidth reduction.

# Render-side numbers

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

# Memory

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
and heap). The game runs four on the window while playing (sky, celestial,
world, arm) plus the UI camera, so folding the sky passes into the world
camera is the next thing to measure.

A release build of the last two rows measured 586 MB with 4x MSAA (GPU 354
MB) and 527 MB without (GPU 316 MB), so the dev profile is not what makes
the footprint large, and the MSAA saving was smaller there, about 40 MB.

## Permanent Beta lighting cleanup

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
