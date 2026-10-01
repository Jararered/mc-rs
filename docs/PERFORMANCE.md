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
