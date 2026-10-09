# Missing Beta 1.7.3 features

Audited 2026-10-08 against the code and `refs/mc_b1.7.3_release/1.7.3-LTS/src/minecraft/net/minecraft/src/`.
This is a static audit (grep and reading), not a play-through, so a "missing" entry means no code path was found.
Each entry names the Beta class to transcribe. Order inside a section is roughly by value for the survival loop.

Implemented since the previous audit and no longer listed: the Nether and portals, redstone (dust, torches, repeaters,
levers, buttons, plates, pistons, dispensers, note-block state), rails and minecarts, beds and spawn points, flint and
steel (recipe, fire, lit TNT, durability), food and soup, double chests, game modes, weather, fire rendering, the bow
and arrow pickup, thrown snowballs and eggs, the fishing rod, boats, riding saddled pigs.

## Items with no use behavior

The items are registered, craftable and stack correctly, but right-click does nothing.

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

- **Boat leftovers** (`EntityBoat`): no `splash` particles from a fast boat (`EntityBoat`; particles now exist, the call is not wired), and a boat is not
  solid to other entities (`getCollisionBox`), so nothing can stand on one.
- **Pig leftovers**: the `flyPig` achievement (needs achievements). Steering is the Pig Steering feature, not Beta.
- **Portals while riding**: a mounted player never touches blocks, so a rider cannot charge a portal; Beta
  dismounts the player inside one (`EntityPlayerSP.onLivingUpdate`).
- **Minecart leftovers** (the last two apply to boats and pigs too): no cart sounds (needs audio), no
  rider yaw drift (`Entity.updateRidden`), and no lying/sitting third-person rider pose (needs the F5 camera).

## Mobs and combat

- **Giant zombie** (`EntityGiantZombie`): not implemented. It never spawns naturally in Beta, so it is only reachable
  through spawn commands or a spawner.
- **Mob-on-mob behavior**: mobs only target the player. Wolves do not hunt sheep, and retaliation between mobs is
  not simulated.
- **Cactus damage to mobs**: only the player and dropped items take it.
- **`performSleepSpawning`** (the monster that wakes a sleeper) is not copied.

## Audio

Nothing makes sound. Bevy's `audio` feature is off and there is no `src/audio/`. Needed: a sound event layer that
gameplay can write to without a renderer, block step/break/place sounds, mob sounds, `fire.ignite`, `random.fuse`,
explosions, ambient cave sounds (`updateBlocksAndPlayCaveSounds`), music, note blocks (state already persists),
jukebox discs, and the volume options. Sound assets must come from the player's own resources, as with textures.

## Particles and presentation hooks

Smoke, flames, lava pops, redstone dust, portal swirls, explosion puffs, bubbles, splashes, hearts and note
particles are implemented (`rendering/particles/`). Still missing: the slime drops (`EntitySlimeFX`) and the
`snowballpoof` burst of a snowball or egg (both item-sprite particles rather than `particles.png`), the fishing-bite
and boat splashes, the wolf shaking spray (wolves do not shake dry yet), and the `spawnExplosionParticle` puffs when
a spawner releases a mob. Creative and spectator players make no water splash.

## Projectile leftovers

- Arrows, thrown items and bobbers are not saved, so a stuck arrow that could be picked up is lost on reload. Beta
  writes `Arrow` (with its `player` flag), `Snowball` and `Egg` entities.
- The fishing line ends at the first-person hand and does not sway with the arm's swing (`RenderFish`); there is no
  third-person line or cast-rod model until the F5 camera exists.
- The Bow Charging feature (Beta 1.8) was written without a 1.8 reference: the bow has no durability, no pull
  icons (they are not in the 1.7.3 `items.png`), and no FOV change.

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
