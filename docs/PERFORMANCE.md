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
skip culled draws (`MULTI_DRAW_INDIRECT_COUNT`). Compare with sun shadows on
and off, since each cascade is another view.

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
