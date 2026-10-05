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

Chunk section layers are plain Bevy meshes, so their GPU memory is held by
Bevy's mesh allocator. The performance report's `mesh slabs` line shows how
many shared vertex and index buffers it holds, their reserved size, and how
many meshes live in them. Compare the reserved size with `mesh memory`, the
bytes chunk layers actually use. A large gap, or slowest-frame spikes while
streaming in a new area, points at `MeshAllocatorSettings` (`min_slab_size`,
`growth_factor`).

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
