# Missing Beta 1.7.3 features

Audited 2026-10-08, re-audited 2026-10-10 and again 2026-10-11 against the code and
`refs/mc_b1.7.3_release/1.7.3-LTS/src/minecraft/net/minecraft/src/` (the client classes are under `.../client/`).
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
- **Dye** (`ItemDye`): the dye recipes work, but two uses do not. Bone meal (damage 15) does nothing. Beta's bone meal
  grows a sapling (`growTree`), fertilizes a crop (`BlockCrops.fertilize`), and spreads tall grass and flowers from
  grass. Dye on a sheep recolors its fleece: `ItemStack.useItemOnEntity` calls `ItemDye.saddleEntity` when the sheep's
  `interact` declines, and an unsheared sheep whose fleece differs takes the dye and loses one. The repo's sheep take
  shears only (`player/interaction/attack.rs`).
- **Jukebox leftovers** (`BlockJukeBox`): a creeper shot by a skeleton drops no disc (mobs do not hurt each other),
  so discs come from dungeon chests and `/give`.
- **Compass, clock and map** (`ItemMap`, `ItemMapBase`, `MapItemRenderer`, `TextureCompassFX`, `TextureWatchFX`): the
  compass and clock have no animated icon, and there is no map item or per-world map data.
- **Carved pumpkin as a helmet** (`SlotArmor.isItemValid`): the helmet slot takes a pumpkin, and a pumpkin worn there
  blurs the first-person view (`GuiIngame.renderPumpkinBlur`, not in the third-person view). `armor_slot` in
  `item/registry.rs` accepts armor items only.
- **Saddle on a hit** (`ItemSaddle.hitEntity`): a melee hit on an unsaddled pig with a saddle in hand saddles it, because
  `EntityPlayer.attackTargetEntityWithCurrentItem` calls `ItemStack.hitEntity` after the damage. The repo saddles on
  right-click only (`player/interaction/attack.rs`).

## Riding

- **Boat leftovers** (`EntityBoat`): a boat is not solid to other entities (`getCollisionBox`), so nothing can stand on
  one. Its fast-boat splashes (`spawnParticle("splash")`) are not spawned, although `FxKind::Splash` exists.
- **Pig leftovers**: the `flyPig` achievement (needs achievements). Steering is the Pig Steering feature, not Beta.
- **Portals while riding**: a mounted player never touches blocks, so a rider cannot charge a portal. Beta dismounts
  the player inside one (`EntityPlayerSP.onLivingUpdate`).
- **Minecart leftovers** (both apply to boats and pigs too): no rider yaw drift
  (`Entity.updateRidden`), and no lying or sitting third-person rider pose (needs the F5 camera).

## Mobs and combat

- **Giant zombie** (`EntityGiantZombie`): not implemented. It never spawns naturally in Beta, so it is only reachable
  through spawn commands or a spawner.
- **Mob-on-mob behavior**: mobs only target the player. Wolves do not hunt sheep, and retaliation between mobs is not
  simulated.
- **Cactus damage to mobs**: only the player and dropped items take it.
- **`performSleepSpawning`** (the monster that wakes a sleeper) is not copied.

## Player rules

- **Peaceful regeneration** (`EntityPlayer.onLivingUpdate`): on Peaceful, a player below 20 health heals one half-heart
  every 20 ticks. The test `ticksExisted % 20 * 12 == 0` is `(ticksExisted % 20) * 12`, so it fires every 20 ticks, not
  every 240. `PlayerHealth::heal` (`player/state.rs`) exists, and only eating food calls it.
- **Apple on death for a player named Notch** (`EntityPlayer.onDeath`): the dead player also drops a red apple. The rule
  reads the username, which the local player does not have, so it waits for named players in multiplayer.

## Audio

Every sound Beta 1.7.3 plays for something the game simulates is played, with background music and the Audio tab
volumes (`src/audio/`, AGENTS' Sound section). Left:

- **Punching a fire out** (`World.onBlockHit`): the fire stays, so aux effect 1004's hiss is never sent.
- **Other players**: one `Sounds` queue is heard from this client's camera. A server has to tell our own client
  what to play; a Beta client makes its own sounds.
- **A resource pack**: the files are read from `resources/` beside `assets/`, in the old launcher's layout. Later
  packs name and arrange them differently (`sound3/`).

## Particles and presentation hooks

All of Beta's `Entity*FX` particles and `randomDisplayTick` effects are implemented (`rendering/particles/`), except the
particles a player gives off while standing in a portal.

- **Spawner cage mob** (`TileEntityMobSpawnerRenderer`): the spawner draws its mob (`mobID`) inside the cage, turning
  slowly, built with `EntityList.createEntityInWorld`. The repo draws the spawner block and its idle smoke and burst
  (`spawner_idle`, `spawner_burst`) only, though `MobSpawner.kind` (`entity/mobs.rs`) already holds the mob.
- **Piston slide** (`TileEntityPiston`, `TileEntityRendererPiston`): pushed blocks slide over a few ticks, and the moving
  piston is drawn in between. `behaviors/piston.rs` moves blocks at once, and the mesher skips `Block::MovingPiston`
  (`rendering/meshing/mesher.rs`), so nothing is drawn mid-move.
- **Title splash text** (`GuiMainMenu`): a random line from `/title/splashes.txt` on the logo, plus the dated greetings.
  The file is a resource, so it waits for the resource pack loader.

## Screen overlays and HUD

Beta draws these from `ItemRenderer.renderOverlays` and `GuiIngame`. None is drawn yet. The portal overlay is drawn
already (`ui/screens/portal.rs`).

- **Armor row** (`GuiIngame`, `getPlayerArmorValue`): ten icons above the hearts, one per two armor points, drawn from
  `icons.png` at x 16, 25 and 34 on row 9 (empty, half and full). `ui/screens/hud.rs` draws the hearts only.
- **Fire in first person** (`ItemRenderer.renderFireInFirstPerson`): the burning overlay while the player is on fire.
- **Block in the eyes** (`renderInsideOfBlock`): the side texture of a solid block covers the screen while the eye is in
  an opaque block (`isEntityInsideOpaqueBlock`). `physics::inside_opaque_block` already answers that.
- **Water tint** (`renderWarpedTextureOverlay`, `misc/water.png`): while the eye is in water. `physics::eye_in_water`
  already answers that, and the fog turns blue underwater (`rendering/sky.rs`), but the screen overlay does not exist.
  Beta has no lava overlay.
- **Vignette** (`GuiIngame.renderVignette`, fancy graphics only): darkens the screen edges by the player's light level
  (`getEntityBrightness`), eased over time.

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
- **Function and fog keys** (`Minecraft.runTick`): the repo has F2 (screenshot), F3 (debug), F4 (wireframe, which Beta
  does not have) and F11 (fullscreen). Beta's other keys are missing:
  - **F1** hides the HUD (`hideGUI`).
  - **F5** switches to a third-person camera (`thirdPersonView`). That unlocks the rider pose and the fishing line's
    swing.
  - **F8** turns on the smooth camera (`smoothCamera`), which filters mouse look in `EntityRenderer`.
  - **F3 with S** reloads resources (`forceReload`): sounds and textures, once the pack loader exists.
  - **F** (`key.fog`) steps the render distance preset (`renderDistance + 1 & 3`: far, normal, short, tiny), and Shift+F
    steps back. The repo's F is flight (`player/controls.rs`), and `change_render_distance` in `app/settings.rs` has no
    caller, so this key needs a decision first.
- **Middle click picks a block** (`Minecraft.clickMiddleMouseButton`): the hotbar slot that holds the aimed block's item
  is selected (`InventoryPlayer.setCurrentItem`), with grass picking dirt, a double slab picking a slab and bedrock
  picking stone. Nothing changes when no hotbar slot holds the item.
- **Controls** (`GuiControls`): the Controls tab has a mouse sensitivity slider only. There is no key rebinding and no
  invert-mouse option (Beta's `GameSettings.invertMouse`).
- **3D anaglyph** (`EnumOptions.ANAGLYPH`): red and cyan stereo. `EntityRenderer` draws each eye from an offset
  (`anaglyphField`), and `FontRenderer`, `RenderEngine`, `RenderBlocks`, `RenderGlobal`, `MapItemRenderer` and the
  texture animations filter their colors while it is on.
- **Texture packs** (`GuiTexturePacks`): no pack screen or ZIP loading. The pack loader is Phase 2 of
  [ROADMAP.md](../../ROADMAP.md).
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

## Checked and not missing

Recorded so the next audit does not look again. Each was compared with the reference.

- Implemented: zombie pigman and its group anger (`entity/mobs.rs`, `player/interaction/attack.rs`), dungeons and their
  loot (`world/generation/overworld/dungeon_loot.rs`), sponges (`behaviors/sponge.rs`, which matches Beta's empty
  `onBlockAdded`), spider wall climbing, slime splitting, sneak edge braking, armor damage, the inventory drop on death,
  and note block tuning.
- Every Beta `Item` constructor has a row in `item/registry.rs`, and the wear a sword or tool takes on a hit is in place
  (`hit_durability`).
- Not in Beta 1.7.3, so nothing to copy: moon phases, `ColorizerWater` (no callers), `GuiSleepMP` (no callers), the Tab
  player list (`GuiIngame` has none), the chest lid animation, an eating animation, a player doll in the inventory,
  sheep grazing, and a wolf shake in the model (`RenderWolf` draws none; the shake is simulation and spray).
- Advanced OpenGL only turns on occlusion queries in `RenderGlobal` (`advancedOpengl && !anaglyph`). Cave culling and
  the chunk culling choice in AGENTS cover that, so the option is not copied.
- Chunk region draw lists are rebuilt on the main thread whenever the camera moves, about 0.7 ms at render distance
  24. Rebuilding only regions whose sections changed visibility, or doing it on a worker, would cut that.
- Frame time with chunk regions and cave culling has not been measured against one entity per section layer: every
  run on the Apple M5 Pro was paced to the display. Measure on a slower GPU or with an unpaced present.
