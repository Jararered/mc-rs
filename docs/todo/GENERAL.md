# Missing Beta 1.7.3 features

Audited 2026-10-08 and re-audited 2026-10-10 against the code and `refs/mc_b1.7.3_release/1.7.3-LTS/src/minecraft/net/minecraft/src/`.
This is a static audit (grep and reading), not a play-through, so a "missing" entry means no code path was found.
Each entry names the Beta class to transcribe. Order inside a section is roughly by value for the survival loop.
The order to build these in is in [ROADMAP.md](../../ROADMAP.md).

Implemented since the previous audit and no longer listed: the Nether and portals, redstone (dust, torches, repeaters,
levers, buttons, plates, pistons, dispensers, note-block state), rails and minecarts, beds and spawn points, flint and
steel (recipe, fire, lit TNT, durability), food and soup, double chests, game modes, weather, fire rendering, the bow
and arrow pickup, thrown snowballs and eggs, the fishing rod, boats, riding saddled pigs, and the Beta 1.7.3 dedicated
server (see Networking).

## Items with no use behavior

These are registered and craftable, but right-click does nothing, or only part of the item works.

- **Painting** (`ItemPainting`, `EntityPainting`, `EnumArt`): no entity, placement or art selection. Beta keeps
  paintings in the chunk's entity list.
- **Sign** (`ItemSign`, `BlockSign`, `TileEntitySign`, `GuiEditSign`): placing and support work
  (`player::place_sign`, `behaviors/attached.rs`), and the four lines save in both formats (`SignText`). Missing: the
  text is not drawn and there is no edit screen, so only text a Beta client wrote shows up.
- **Cake** (`ItemReed` for the cake item, `BlockCake`): the block is not placeable or drawn (`Block::placed` returns
  `None`), and there is no eating. Six slices; each heals 3 while health is below 20 (`BlockCake.eatCakeSlice`).
- **Dye** (`ItemDye`): the dye recipes work, but bone meal (damage 15) does nothing. Beta's bone meal grows a sapling
  (`growTree`), fertilizes a crop (`BlockCrops.fertilize`), and spreads tall grass and flowers from grass. Dye does not
  recolor a sheep in Beta 1.7.3; wool is dyed by crafting only.
- **Records and jukebox** (`ItemRecord`, `BlockJukeBox`, `TileEntityRecordPlayer`): the discs and the jukebox block
  exist, but there is no insert, eject or playback. Beta saves the disc as `Record`. Playback needs audio.
- **Compass, clock and map** (`ItemMap`, `ItemMapBase`, `MapItemRenderer`, `TextureCompassFX`, `TextureWatchFX`): the
  compass and clock have no animated icon, and there is no map item or per-world map data.

## Riding

- **Boat leftovers** (`EntityBoat`): a boat is not solid to other entities (`getCollisionBox`), so nothing can stand on
  one. Its fast-boat splashes (`spawnParticle("splash")`) are not spawned, although `FxKind::Splash` exists.
- **Pig leftovers**: the `flyPig` achievement (needs achievements). Steering is the Pig Steering feature, not Beta.
- **Portals while riding**: a mounted player never touches blocks, so a rider cannot charge a portal. Beta dismounts
  the player inside one (`EntityPlayerSP.onLivingUpdate`).
- **Minecart leftovers** (the last two apply to boats and pigs too): no cart sounds (needs audio), no rider yaw drift
  (`Entity.updateRidden`), and no lying or sitting third-person rider pose (needs the F5 camera).

## Mobs and combat

- **Giant zombie** (`EntityGiantZombie`): not implemented. It never spawns naturally in Beta, so it is only reachable
  through spawn commands or a spawner.
- **Mob-on-mob behavior**: mobs only target the player. Wolves do not hunt sheep, and retaliation between mobs is not
  simulated.
- **Cactus damage to mobs**: only the player and dropped items take it.
- **`performSleepSpawning`** (the monster that wakes a sleeper) is not copied.

## Audio

Nothing makes sound. Bevy's `audio` feature is off in `Cargo.toml`, and there is no `src/audio/`. Needed: a sound
message that the simulation can write without a renderer; block step, break and place sounds (`StepSound`); mob
sounds; `fire.ignite`; `random.fuse`; explosions; fluid flow; ambient cave sounds (`updateBlocksAndPlayCaveSounds`);
rain and thunder; background music; note block pitch (the state already persists, `behaviors/note.rs`); jukebox discs;
portal sounds; and the sound and music volume options. Sound assets must come from the player's own resources, as
textures will.

## Particles and presentation hooks

All of Beta's `Entity*FX` particles and `randomDisplayTick` effects are implemented (`rendering/particles/`), except the
particles a player gives off while standing in a portal (AGENTS' Dimensions section). A wolf shakes dry and sprays, but
its model does not play the shake animation (`RenderWolf`).

## Projectile leftovers

- Arrows, thrown items and bobbers are not saved, so a stuck arrow that could be picked up is lost on reload. Beta
  writes `EntityArrow` (with its `player` flag), `EntitySnowball`, `EntityEgg` and `EntityFish`.
- The fishing line ends at the first-person hand and does not sway with the arm's swing (`RenderFish`); there is no
  third-person line or cast-rod model until the F5 camera exists.
- The Bow Charging feature (Beta 1.8) was written without a 1.8 reference: the bow has no durability and no pull icons
  (they are not in the 1.7.3 `items.png`).

## Client UI and controls

- **Death screen** (`GuiGameOver`): the player respawns automatically after the death animation. There is no screen
  with the score and the Respawn and Title buttons. The score (`EntityPlayer.score`) is not tracked, and `original.rs`
  drops it.
- **F1 and F5**: no hide-HUD toggle and no third-person camera. Only F2 (screenshot), F3 (debug), F4 (wireframe) and
  F11 (fullscreen) exist.
- **Controls** (`GuiControls`): the Controls tab has a mouse sensitivity slider only. There is no key rebinding and no
  invert-mouse option (Beta's `GameSettings.invertMouse`).
- **Sound and music volume** options (`GuiOptions`) need audio first.
- **Texture packs** (`GuiTexturePacks`): no pack screen or ZIP loading. AGENTS plans for players to supply a ZIP.
- **Statistics and achievements** (`GuiStats`, `GuiAchievements`, the `GuiAchievement` popup, `StatList`,
  `AchievementList`): no tracking, screens or popup. Beta saves each player's stats as `stats_<name>.dat` beside the
  world, so both formats need them.
- **Rename world** (`GuiRenameWorld`): the world list can delete worlds, but there is no rename. Beta keeps the name as
  `LevelName` in `level.dat`.
- **Multiplayer screens** (`GuiMultiplayer`, `GuiConnecting`, `GuiConnectFailed`): see Networking.

## Networking

The Beta 1.7.3 server exists. `src/networking/beta/` speaks protocol 14 (login, chunks with light, movement, block
changes, chat, windows, health, time, other players), and `cargo run --bin server` runs it. What it still lacks, and
the order to add it in, is in [MULTIPLAYER.md](MULTIPLAYER.md), not here.

Not started: drawing other players in the game (`EntityOtherPlayerMP`), the join screen (see Client UI), our own
client's protocol and transport, and a Beta protocol client that joins original servers (optional).
