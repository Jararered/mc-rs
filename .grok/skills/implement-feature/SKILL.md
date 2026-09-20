---
name: implement-feature
description: Implement a gameplay or rendering feature in this Minecraft Beta-inspired Bevy game. Decide whether it needs a togglable `GameSettings` field and, if so, wire the settings menu and tests. Remesh only when the feature actually depends on it (example: Fancy/transparent leaves). Use when the user asks to add a feature, implement a Beta option, add a graphics toggle, or runs `/implement-feature`.
---

# Implement a game feature

Follow `Agents.md`. Put working behavior in the matching subsystem under `src/`; put tests under `tests/`. Do not add empty modules.

## 1. Decide if it needs a setting

Read `src/app/settings.rs` and `src/ui/mod.rs` before adding anything.

**Add a `GameSettings` field** when the feature is a player-facing mode with two (or more) valid behaviors: graphics quality, lighting style, view distance, or a Beta options-screen toggle. Transparent leaves is the template: Fancy uses cutout tiles and unculled canopy faces; Fast uses opaque tiles and solid culling. That lives on `fancy_graphics`.

**Do not add a setting** when the feature is just how the game works (tree decoration, world saves, player pose). Implement it; skip menu wiring.

If unsure, prefer no toggle unless the user asked for one or Beta 1.7.3 exposes it as an option.

## 2. Implement the feature

Ship the behavior first. Match existing Rust/Bevy style. Keep generation, lighting, meshing, and streaming distinct.

When the setting exists, **read it where the behavior is decided** (meshing, materials, lighting, streaming). Copy it onto `WorldStreaming` only if streaming jobs or remesh need a stable snapshot.

## 3. Wire a toggle (only if step 1 said yes)

Update all of these together:

1. `src/app/settings.rs` — field, `Default`, clamp/toggle helper if it is a range.
2. `src/ui/mod.rs` — `MenuAction`, `SettingLabel`, spawn the control on the settings screen, handle the press, refresh the label text.
3. The system that applies the setting (for example `apply_lighting_settings` or terrain `alpha_mode`).

Bool toggles flip in `handle_buttons` and format like existing labels (`ON`/`OFF`, or `Fancy`/`Fast`).

## 4. Remesh only when the feature needs it

Chunk meshes are rebuilt from `WorldStreaming.remesh_queue` when a copied setting on `WorldStreaming` disagrees with `GameSettings` (`src/world/streaming/mod.rs`).

Add that compare-and-queue path **only if** changing the setting changes UVs, face culling, vertex colors, or other mesh data. Fancy leaves do. Ambient brightness does not — it updates lights and skips remesh.

Material-only changes (for example `AlphaMode`) update the `StandardMaterial` in place; still remesh if tile UVs or culling also change.

## 5. Tests

- Settings ranges/defaults and the new toggle: `tests/settings.rs`.
- If the setting changes engine state (lights, materials), drive a headless `App` with `WorldPlugin` and assert the result, same as `brightness_and_directional_toggle_update_bevy_lights`.
- Feature behavior (meshing, generation, persistence) in the matching file under `tests/`. For a graphics mode, assert both modes (Fast vs Fancy leaf face counts and atlas tiles).
- Run `cargo fmt` and the tests you touched.

## Checklist

- [ ] Feature works without a setting, or the setting is justified
- [ ] Menu action, label, and spawn exist iff there is a setting
- [ ] Remesh queued iff meshes depend on the setting
- [ ] Tests cover the feature and any toggle
