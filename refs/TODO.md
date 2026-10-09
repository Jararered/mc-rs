# Missing Beta 1.7.3 features

Audited 2026-10-08 against the code and `refs/mc_b1.7.3_release/1.7.3-LTS/src/minecraft/net/minecraft/src/`.
This is a static audit (grep and reading), not a play-through, so a "missing" entry means no code path was found.
Each entry names the Beta class to transcribe. Order inside a section is roughly by value for the survival loop.

Implemented since the previous audit and no longer listed: the Nether and portals, redstone (dust, torches, repeaters,
levers, buttons, plates, pistons, dispensers, note-block state), rails and minecarts, beds and spawn points, flint and
steel (recipe, fire, lit TNT, durability), food and soup, double chests, game modes, weather, fire rendering.

## Items with no use behavior

The items are registered, craftable and stack correctly, but right-click does nothing.

- **Bow and arrows** (`ItemBow`, `EntityArrow`): no charge, draw animation or shot; no arrow pickup. Skeletons and
  dispensers already fire `entity::projectiles::Arrow`, so the entity exists. Needs a player-owned arrow that can be
  picked up and may trigger wooden plates.
- **Snowball and egg** (`ItemSnowball`, `EntitySnowball`, `ItemEgg`, `EntityEgg`): no throwing entity. Eggs have a
  1/8 chance to hatch a chicken; a snowball only knocks back in Beta. Dispensers eject both as plain items instead of
  launching them.
- **Fishing rod** (`ItemFishingRod`, `EntityFish`): no bobber, catch roll or reel-in.
- **Boat** (`ItemBoat`, `EntityBoat`): no entity, placement, riding or drop.
- **Painting** (`ItemPainting`, `EntityPainting`, `EnumArt`): no entity, placement or art selection.
- **Sign** (`ItemSign`, `BlockSign`, `TileEntitySign`, `GuiEditSign`): the blocks have no shape, text storage or edit
  screen, and `Block::placed` returns `None` for them.
- **Cake** (`ItemReed` for the cake item, `BlockCake`): the block is not placeable or drawn, and there is no eating
  (six slices, heals 3 per slice).
- **Dye** (`ItemDye`, `EntitySheep.interact`): neither bone meal nor sheep dyeing is wired up. Bone meal should
  grow crops and saplings; dye on a sheep recolors its wool.
- **Records and jukebox** (`ItemRecord`, `BlockJukeBox`, `TileEntityRecordPlayer`): no insert, eject or playback.
- **Compass, clock and map** (`ItemMap`, `ItemMapBase`, `RenderItem` icon animation): no animated compass or clock
  icon, and no map item, map data or map rendering.

## Riding

- **Saddled pigs** (`EntityPig.interact`, `Entity.mountEntity`): saddling works, mounting and steering do not. The
  mount mechanism (`entity/mount.rs`: `Mounted`, `snap_riders`, `release_orphans`) is general; a pig needs its own
  `getMountedYOffset` (`height * 0.75`, `EntityPig`) and steering from the rider's look.
- **Boats** (see above) can reuse the same mount.
- **Minecart leftovers**: no furnace-cart smoke (`largesmoke`, needs particles), no cart sounds (needs audio), no
  rider yaw drift (`Entity.updateRidden`), and no lying/sitting third-person rider pose (needs the F5 camera).

## Mobs and combat

- **Giant zombie** (`EntityGiantZombie`): not implemented. It never spawns naturally in Beta, so it is only reachable
  through spawn commands or a spawner.
- **Mob-on-mob behavior**: mobs only target the player. Wolves do not hunt sheep, and retaliation between mobs is
  not simulated.
- **Cactus damage to mobs**: only the player and dropped items take it.
- **Arrows on wooden pressure plates**: plates read the per-tick body snapshot, which does not include arrows.
- **`performSleepSpawning`** (the monster that wakes a sleeper) is not copied.

## Audio

Nothing makes sound. Bevy's `audio` feature is off and there is no `src/audio/`. Needed: a sound event layer that
gameplay can write to without a renderer, block step/break/place sounds, mob sounds, `fire.ignite`, `random.fuse`,
explosions, ambient cave sounds (`updateBlocksAndPlayCaveSounds`), music, note blocks (state already persists),
jukebox discs, and the volume options. Sound assets must come from the player's own resources, as with textures.

## Particles and presentation hooks

`entity/particles/` has only block break/hit and rain splashes. Missing, from `Block.randomDisplayTick` and the
`Entity*FX` classes: smoke and flames (torches, furnaces, fire), lava pops and drips, redstone dust and ore sparkles,
portal swirl, explosion and large-smoke puffs, bubbles and splash, hearts when breeding or taming, note particles,
snow-shovel puffs and slime drops.

## Client UI and controls

- **Death screen** (`GuiGameOver`): the player respawns automatically after the death animation. There is no screen
  with the score and the Respawn and Title buttons.
- **F1 and F5**: no hide-HUD toggle and no third-person camera. Only F2 (screenshot), F3 (debug), F4 and F11 exist.
- **Controls screen** (`GuiControls`): no key rebinding, and I found no invert-mouse option.
- **Sound and music volume** options (`GuiOptions`) need audio first.
- **Texture packs** (`GuiTexturePacks`): no pack screen or ZIP loading. AGENTS.md plans for players to supply a ZIP.
- **Statistics and achievements** (`GuiStats`, `GuiAchievements`, `StatList`, `AchievementList`): no tracking,
  screens or on-screen achievement popup.
- **Rename world** (`GuiRenameWorld`): the world list can delete worlds, and I found no rename.
- **Connect, Multiplayer and error screens** (`GuiMultiplayer`, `GuiConnecting`, `GuiConnectFailed`): see Networking.

## Networking

`src/networking/` does not exist. No server, LAN play, remote-player entities (`EntityOtherPlayerMP`) or Beta
protocol adapter. Per AGENTS.md this is a later priority, so keep protocol concerns out of the core simulation.
