# Roadmap

What is left to build, in the order to build it. It starts from the 2026-10-10 re-audit in
[docs/todo/GENERAL.md](docs/todo/GENERAL.md) and the multiplayer list in
[docs/todo/MULTIPLAYER.md](docs/todo/MULTIPLAYER.md). [AGENTS.md](AGENTS.md) sets the order: a playable Beta
singleplayer game comes first, and Beta world and server compatibility comes last. Multiplayer is already under way,
so it runs alongside the phases below rather than after them.

Tick a step when it lands, as the TODO files do. Each step names the Beta class to transcribe.

## How each step lands

1. Transcribe the behavior from `refs/mc_b1.7.3_release/1.7.3-LTS/src/minecraft/net/minecraft/src/`, grepping for the
   methods involved. Do not read whole files.
2. Put static facts in `data/*.ron`, and block updates in `src/world/block_ticks/behaviors/` (see
   [docs/BLOCK_TICKS.md](docs/BLOCK_TICKS.md)).
3. Save any world-scoped state in both formats, or document in `original.rs` that it is lost.
4. Add tests under `tests/`, mirroring `src/`.
5. Add a line to the CHANGELOG's Unreleased entry, marked `pending` until it is committed.

## Phase 0: Housekeeping

- [x] **Documentation.** Each doc has one job, mapped in [docs/README.md](docs/README.md). `AGENTS.md` keeps the rules.
      Subsystem guides live in `docs/`. The dangling `docs/PLAN.md` and `.grok/skills/implement-feature/SKILL.md`
      references are gone. The feature-toggle rule now lives in `AGENTS.md`.

## Phase 1: Finish the survival loop

Each step already has its data, block or item in place, and needs no audio.

- [ ] **Death screen.** `GuiGameOver`: the score line, and Respawn and Title buttons. The respawn waits for the
      button, as a remote player's already does (`ClientRespawn`). The score is Beta's `EntityPlayer.score`, which the
      game does not track yet and `original.rs` drops, so it goes into both formats.
- [ ] **Cake.** Place it with the box already in `data/blocks.ron`. Right-click eats one of six slices, healing 3 while
      health is below 20 (`BlockCake`). The last slice removes the block. `Block::placed` needs the cake case.
- [ ] **Sign text.** `GuiEditSign` for the four lines, and draw them on the sign faces. Storage and both formats
      already work (`SignText`), so this is UI and mesh work.
- [ ] **Bone meal.** `ItemDye` at damage 15 on a sapling (`growTree`, through the `TreeWorld` trait the tree generators
      share), on a crop (`fertilize`), and on grass (the 128-try spread of tall grass and flowers).
- [x] **Jukebox storage.** Right-click inserts a disc and right-click again ejects it. Breaking the block drops the
      disc. The state is a `TileEntityRecordPlayer` holding `Record`, saved in both formats. Playback is
      `audio::records`.
- [ ] **Projectiles in saves.** Save arrows (with their `player` flag), snowballs, eggs and bobbers in both formats, so
      a stuck arrow a player could pick up survives a reload.
- [ ] **Painting entity.** `EntityPainting` and `EnumArt`: place on a wall that fits the art, drop the item when it
      breaks, and save it with its chunk in both formats. The art needs a source, so either wait for Phase 2 or draw a
      flat placeholder (see open questions).
- [ ] **Compass and clock icons.** Choose the icon frame per stack: the compass needle points at the world spawn, and
      the clock shows `world_time` (Beta's `TextureCompassFX` and `TextureWatchFX`). Write the frames from the render
      world, as the water tiles are, rather than editing the atlas.

## Phase 2: Resource packs

Paintings and textures still come from `assets/`, which is reference-only and goes away before release (AGENTS).
Sounds already play from the player's `resources/` folder. One loader should replace both, so textures and sounds
are read the same way.

- [ ] **Pack loader.** Read a Beta-layout pack from a ZIP or a folder: `terrain.png`, `misc/`, `gui/`, `font/`, `art/`,
      `sound/` and `music/`. The game must still start with no pack, as it does now.
- [ ] **Texture packs screen** (`GuiTexturePacks`). List packs, select one and apply it. Applying rebuilds the terrain
      atlas once, not every frame.
- [ ] **Read everything through the loader.** Once terrain, font, GUI and art all go through it, the game stops reading
      `assets/` directly.

## Phase 3: Audio

The sound path is in place. The simulation asks for a sound by name through `world::sound::Sounds`, and `src/audio/`
plays it, so a headless server needs no audio plugin. What Beta plays for something the game already simulates is
played, apart from the leftovers in [docs/todo/GENERAL.md](docs/todo/GENERAL.md) (punching a fire out, and sound for
any player but this client's). Files are read from the player's `resources/` folder. A pack loader is Phase 2.

- [x] **Foundation.** Bevy's audio feature, an Ogg decoder, `Sounds`, and `src/audio/`. With no `resources/` folder the
      game plays silence and still runs.
- [x] **Volume options.** A master and a per-category volume on the Audio tab (`SoundCategory`). Beta had Sound and
      Music only.
- [x] **Block sounds.** Step, dig, place and break sounds from each block's `StepSound`.
- [x] **Mob, explosion and fire sounds.** Living, hurt and death sounds per mob, the player's hurt and death sounds,
      the bow's shot, `random.fuse` for primed TNT, explosions, and fire.
- [x] **Ambient and weather.** Cave ambience, rain and thunder, fluid flow, and portal sounds.
- [x] **Music.** Background music on the title screen and in game, from `resources/music/` and `newmusic/`.
- [x] **Note blocks and jukebox.** Note pitch from the stored state (`behaviors/note.rs`), and the disc in a jukebox
      (`audio::records`).
- [x] **Vehicle sounds.** A pig's voice is in `data/sounds.ron`. Beta 1.7.3's `EntityMinecart` and `EntityBoat` never
      call `playSound`. The boat's fast splash is the particle in Phase 4.

## Phase 4: Riding, vehicles and portals

Beta's classes for these are already transcribed, so this phase finishes them.

- [ ] **Boat collision box.** Make a boat solid to other entities through `getCollisionBox`, so a body can stand on one.
      `bump_boats` already covers the push.
- [ ] **Fast-boat splash.** Spawn `FxKind::Splash` where Beta's `EntityBoat` spawns `"splash"`. The kind exists; the
      boat does not call it yet.
- [ ] **Portals while riding.** A rider in a portal is dismounted, so the player can charge it
      (`EntityPlayerSP.onLivingUpdate`). The work is in `player/portal.rs` and `entity/mount.rs`.
- [ ] **Rider yaw drift.** `Entity.updateRidden` for carts, boats and pigs.
- [ ] **Stray Nether portal.** The portal Beta can leave when a player who died in the Nether is sent back through the
      teleporter (`world::portal`).
- [ ] **Rider pose.** The lying and sitting third-person pose. It needs the third-person camera from Phase 7.

## Phase 5: Mobs and combat

- [ ] **Cactus contact for mobs.** Mobs take cactus damage through the same contact check the player and dropped items
      use (`touches_cactus`), in `entity/combat.rs`.
- [ ] **Mob targets beyond the player.** `Living::target` holds only players today. Let a mob hit by another mob turn on
      its attacker (`EntityLiving`), and let wolves hunt sheep. This also covers the angered-mob item in
      MULTIPLAYER.md.
- [ ] **`performSleepSpawning`.** A monster wakes a sleeping player, as in Beta.
- [ ] **Giant zombie.** `EntityGiantZombie`, with its scaled model and stats. It is reachable only through a spawner or a
      spawn command, as in Beta, so it comes last.

## Phase 6: Statistics, achievements and maps

- [ ] **Stats tracking.** `StatList` counters on the player (blocks, items, distance and time). Save them per player as
      `stats_<name>.dat` in both formats.
- [ ] **Achievements and popup.** `AchievementList` with its unlock rules and the on-screen popup (`GuiAchievement`).
      The `flyPig` achievement needs the riding work in Phase 4.
- [ ] **Statistics and achievements screens.** `GuiStats` (general, block and item tabs) and `GuiAchievements`.
- [ ] **Map item.** `ItemMap` and `ItemMapBase`, with the per-map data (`MapData`) drawn by `MapItemRenderer` from the
      chunks around its holder. Saved per world in both formats.

## Phase 7: Client UI and controls

- [ ] **Key rebinding.** `GuiControls` with a key table in the saved settings, plus the invert-mouse option (Beta's
      `GameSettings.invertMouse`).
- [ ] **F1 and F5.** F1 hides the HUD. F5 switches to a third-person camera, which unlocks the rider pose (Phase 4) and
      the fishing line's swing (`RenderFish`).
- [ ] **Rename world.** `GuiRenameWorld` from the world list. It writes `LevelName` in `level.dat`, and the native name
      in the native format.

## Phase 8: Multiplayer (runs alongside Phases 1 to 7)

The Beta server is in place, and the rest is listed in [docs/todo/MULTIPLAYER.md](docs/todo/MULTIPLAYER.md). Suggested
order:

- [ ] **Entities the original client must see.** Mobs, dropped items, vehicles, projectiles and riding
      (`Packet23VehicleSpawn`, `Packet39AttachEntity`). Without them the original client sees a world with no mobs.
- [ ] **Windows and signs.** Container windows and their clicks (`Packet100OpenWindow`, `Packet102WindowClick` answered
      with `Packet106Transaction`), signs (`Packet130UpdateSign`), note blocks and doors.
- [ ] **Weather, sleep and explosions.** `Packet71Weather`, `Packet70Bed`, `Packet17Sleep` and `Packet60Explosion`.
- [ ] **Paintings on the wire.** `Packet25EntityPainting` and a tracker row, once Phase 1's painting exists.
- [ ] **Two original clients.** The open checks in MULTIPLAYER.md, then its trust and robustness list.
- [ ] **Our own client.** Draw other players, the join screen (`GuiMultiplayer`, `GuiConnecting`,
      `GuiConnectFailed`), and the remote-world mode. The server split (light without meshes, generation separated from
      meshing) goes with it.

## Phase 9: Beta compatibility (last)

AGENTS puts this last. Start it after the phases above, and keep it at the adapter boundary.

- [ ] **Round trip for new state.** Check that everything Phases 1 to 6 add (paintings, projectiles, jukebox discs, sign
      text, stats, maps) loads and saves in Beta's format, or is documented as lost in `original.rs`.
- [ ] **Entity and window packets against the original client.** None has been tried yet (MULTIPLAYER.md).
- [ ] **Beta protocol client** (optional): join original Beta servers.

## Not in Beta 1.7.3

Checked against the reference classes. These are later features and stay out of scope:

- Spawn eggs (there is no `ItemMonsterPlacer`), endermen, ravines, villages, mineshafts, strongholds and the enchanting
  table.

## Open questions

1. **Resource pack format.** Read ZIPs first (which needs a crate) or folders first? This decides the Phase 2 dependency.
2. **Paintings before a pack exists.** Draw placeholder paintings, or hold the painting entity until Phase 2? Sounds
   already play from the player's `resources/` folder.
3. **Multiplayer pace.** Keep it running alongside Phases 1 to 7, or pause it until the singleplayer phases land? AGENTS
   puts Beta-world compatibility last, but the server is already built.
4. **Death screen score.** Build the score counter with the death screen in Phase 1, or leave the score line out until
   statistics exist in Phase 6?

## Checks at phase ends

- `cargo test`, plus a `cargo run` pass over the phase's CHANGELOG items.
- After a change to audio or to mobs (Phase 5), measure with `EntityDiagnostics` in `cargo run --release`, as AGENTS
  asks for performance-sensitive work.
- Before Phase 9, re-audit [docs/todo/GENERAL.md](docs/todo/GENERAL.md) the same way it was done here.
