# Block ticks

Blocks that change on their own — water and lava flowing, sand falling,
grass spreading, crops growing, leaves decaying, redstone ore dimming — run
through one system in `src/world/block_ticks/`. It is Beta 1.7.3's update
machinery: `World.scheduleBlockUpdate` / `TickUpdates`, the random ticks of
`updateBlocksAndPlayCaveSounds`, `notifyBlocksOfNeighborChange`, and the
`Block` hooks they call (`updateTick`, `onNeighborBlockChange`,
`onBlockAdded`, `onBlockRemoval`, ...). When in doubt, read the matching
`Block*.java` under `refs/` and transcribe it.

This guide explains how the pieces fit and how to give a block new update
behavior.

## Where things live

| Path | What it holds |
|------|---------------|
| `src/world/block_ticks/mod.rs` | `BlockTicks` resource, `BlockEvent`, `BlockChange`, `TickEffect`, constants |
| `src/world/block_ticks/behavior.rs` | The `BlockBehavior` trait and the lookup `behavior(id)` |
| `src/world/block_ticks/behaviors/mod.rs` | The registration table: which blocks use which behavior |
| `src/world/block_ticks/behaviors/*.rs` | One module per block family (fluid, falling, soil, crops, plants, leaves, ore, snow, attached, sponge) |
| `src/world/block_ticks/world.rs` | `TickWorld`, the world view a behavior reads and writes |
| `src/world/block_ticks/scheduler.rs` | The scheduled tick queue |
| `src/world/block_ticks/systems.rs` | ECS systems and `BlockTicksPlugin` |
| `src/block/fluids.rs` | Fluid level math shared with meshing and physics |
| `src/entity/falling_block.rs` | The falling sand/gravel entity |
| `src/world/chunk/chunk.rs` | Per-block metadata and pending tick storage |
| `src/world/lighting/mod.rs` | `LightCache`, the light ticks read |
| `tests/world/block_ticks/` | Tests, plus the `TestWorld` harness |

## What happens each world tick

`run_block_ticks` runs in `BlockTickSet`, after player input and physics and
before streaming. For every 20 Hz tick the frame consumed:

1. **Events.** Pending `BlockEvent`s run first (once per frame). Code outside
   the tick pass that edits the world reports what it did here.
2. **Scheduled ticks.** Up to 1000 due entries run in due order, then
   scheduling order. An entry only runs if the cell still holds the block it
   was scheduled for, and only once the chunks 8 blocks around it are
   loaded; otherwise it waits for the next tick.
3. **Random ticks.** Each finished, lit chunk within 9 chunks of the player
   (capped by the render distance) gets 80 random cells from Beta's LCG.
   Cells whose block `ticks_randomly` run `update_tick`. Each chunk also
   gets the snowy-biome ice-freezing roll.

Afterwards the system turns the tick pass's `BlockChange`s into remesh and
relight requests for streaming and dirty chunks for persistence, and
`apply_tick_effects` spawns item drops and falling blocks.

## The `BlockBehavior` trait

Every hook has a no-op default. Implement only what the block reacts to.

| Hook | Beta | Runs when |
|------|------|-----------|
| `ticks_randomly(block)` | `Block.tickOnLoad` | Whether random ticks reach this block |
| `tick_rate(block)` | `Block.tickRate()` | The delay the block usually schedules itself with |
| `update_tick(world, pos)` | `Block.updateTick` | A scheduled tick comes due, or a random tick lands (if `ticks_randomly`) |
| `neighbor_changed(world, pos, neighbor)` | `onNeighborBlockChange` | A face neighbor changed and notified |
| `on_added(world, pos)` | `onBlockAdded` | The block was just written into the world |
| `on_removed(world, pos, previous, metadata)` | `onBlockRemoval` | The block was just replaced (the world already holds the new block) |
| `harvested(world, pos, block, metadata)` | `harvestBlock`'s world side | A player broke it with a tool that can harvest it (ice leaving water) |
| `clicked(world, pos)` | `onBlockClicked` | The player started mining it |
| `activated(world, pos)` | `blockActivated` | The player right-clicked it |
| `entity_walked(world, pos)` | `onEntityWalking` | An entity took a step on it |

Behaviors are stateless unit structs with a `static` instance. One behavior
serves every torch facing and every leaf species, since those live in metadata;
read which one with `world.metadata(pos)`. As in Beta, random and scheduled
ticks call the same `update_tick`.

## `TickWorld`: the world a behavior sees

Reads outside the world or in an unloaded chunk see air and metadata 0,
like Beta's `getBlockId`. Writes there do nothing. Guard long-reaching
searches with `area_loaded` the way the Java does with `checkChunksExist`.

| Beta `World` | `TickWorld` |
|--------------|-------------|
| `getBlockId`, `getBlockMetadata`, `isAirBlock` | `block`, `metadata`, `is_air` |
| `checkChunksExist(r)` | `area_loaded(pos, r)` |
| `isBlockOpaqueCube`, `isBlockNormalCube`, `getBlockMaterial().isSolid()` | `is_opaque_cube`, `is_normal_cube`, `is_solid` |
| `getBlockLightValue`, `getFullBlockLightValue` | `light`, `full_light` |
| `getSavedLightValue(Block/Sky)` | `block_light`, `sky_light` |
| `canBlockSeeTheSky`, `canBlockBeRainedOn`, `findTopSolidBlock` | `sees_sky`, `rained_on`, `top_solid_block` |
| `BiomeGenBase.getEnableSnow` | `snows_at(x, z)` |
| `setBlock`, `setBlockAndMetadata`, `setBlockMetadata` | `set_block`, `set_block_and_metadata`, `set_metadata` |
| `setBlockWithNotify`, `setBlockAndMetadataWithNotify`, `setBlockMetadataWithNotify` | `set_block_notify`, `set_block_and_metadata_notify`, `set_metadata_notify` |
| `notifyBlocksOfNeighborChange` | `notify_neighbors(pos, block)` |
| `scheduleBlockUpdate(x, y, z, id, delay)` | `schedule(pos, block, delay)` |
| `editingBlocks`, `scheduledUpdatesAreImmediate` | `set_editing`, `set_immediate` |
| `World.rand` (the `Random` every `updateTick` gets) | `random()` |
| `Block.dropBlockAsItem` | `drop_block_as_item(pos, block, metadata)` |
| `new EntityFallingSand` | `spawn_falling_block(pos, block)` |

Hook semantics follow Beta: `set_block*` run the old block's `on_removed` and
the new block's `on_added`; only the `*_notify` variants notify neighbors.
`set_block` counts writing the same block as no change, while
`set_block_and_metadata` counts a metadata change as one. Neighbor
notifications nest like Beta's call stack; past 64 levels they are queued
and delivered once the current update unwinds.

Light comes from `LightCache`, the light streaming computed at each chunk's
last mesh job, so it lags an edit by a mesh job. Light is Beta's scale:
0–15 per channel. Opacity from `block::definition::light_opacity` uses the
same numbers as Beta's `Block.lightOpacity` below 15, so thresholds such as
grass's `> 2` transcribe directly.

## Metadata

Chunks store Beta's 4-bit per-block metadata (`Chunk::metadata`,
`WorldChunks::metadata_at`), allocated only once a chunk holds a nonzero
value, and saved in chunk files. Writing a different block resets it to 0,
as `setBlockID` does. Species and facing live there too, as in Beta, so a
block id never encodes state. `Block::facing` and `Block::facing_metadata`
convert facings, and `blocks::species` names the species values.

| Block | Metadata |
|-------|----------|
| Water, lava | Flow decay: 0 source, 1–7 spread, 8+ falling |
| Crops | Growth stage 0–7 |
| Farmland | Moisture 0–7 (wet texture when > 0) |
| Cactus, sugar cane | Growth counter 0–15 |
| Redstone wire | Power level 0–15 |
| Levers, buttons | Support side in bits 0–2, powered bit 8 |
| Repeaters | Facing in bits 0–1; delay in bits 2–3 |
| Doors, trapdoors | Facing, open bit 4, upper-half bit 8 (doors) |
| Pistons | Facing 0–5 and extended bit 8 |
| Rails | Track shape 0–9; powered and detector rails keep the power bit 8 |
| Leaves | Bits 0-1 species (oak, spruce, birch); bit 8 (`CHECK_DECAY`): look for a log on the next random tick |
| Wood, planks | Bits 0-1 species |
| Tall grass | 2 is the fern |
| Torch | 0 floor; 1 west, 2 east, 3 north, 4 south wall |
| Ladder | 2 south, 3 north, 4 east, 5 west wall |
| Furnace, lit furnace, chest | 2 north, 3 south, 4 west, 5 east front |
| Pumpkin | 0 west, 1 south, 2 east, 3 north front |

## Adding update behavior to a block

Say a new block needs Beta's `BlockFoo.updateTick`.

1. **Find the Java.** `refs/.../src/minecraft/net/minecraft/src/BlockFoo.java`.
   Note `setTickOnLoad`, `tickRate`, and every overridden hook.
2. **Write the behavior** in the family's module under
   `src/world/block_ticks/behaviors/`, or a new module (declare it in
   `behaviors/mod.rs`):

   ```rust
   /// Beta `BlockFoo`: what it does, in a sentence.
   pub struct Foo;
   pub static FOO: Foo = Foo;

   impl BlockBehavior for Foo {
       fn ticks_randomly(&self, _block: Id) -> bool {
           true // `setTickOnLoad(true)`
       }

       fn on_added(&self, world: &mut TickWorld, position: IVec3) {
           let block = world.block(position);
           world.schedule(position, block, self.tick_rate(block));
       }

       fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Id) {
           if !world.is_normal_cube(position - IVec3::Y) {
               let metadata = world.metadata(position);
               world.drop_block_as_item(position, Id::Foo, metadata);
               world.set_block_notify(position, Id::Air);
           }
       }

       fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
           if world.light(position + IVec3::Y) >= 9 && world.random().next_int(10) == 0 {
               let age = world.metadata(position);
               world.set_metadata_notify(position, age + 1);
           }
       }
   }
   ```

   Keep the Java's order of random draws and its `WithNotify` choices; they
   decide what neighbors see. Name Beta methods and fields in doc comments.
3. **Register it** in `behaviors::table()`:
   `register(&mut table, &[Id::Foo], &foo::FOO);` List every compact value
   that shares the behavior.
4. **If metadata changes how it looks**, teach the mesher to draw it and
   update `meshing::same_appearance`, which decides whether a change needs a
   remesh. Metadata the mesher ignores (ages, flags) needs nothing.
5. **If it drops something that depends on metadata**, extend
   `entity::drops::blocks::push_natural`.
6. **Test it** in `tests/world/block_ticks/` with `TestWorld`: build blocks
   with `set`, trigger hooks with `place` (a player-style edit) or `event`,
   run scheduled ticks with `run(n)`, force random ticks on a cell with
   `random_ticks(pos, n)`, light the world with `relight()`, and inspect
   `drops()`, `effects()`, and `changes()`.

A block that is not in the world yet (`Id::in_world` is false) also needs
its definition, placement, and rendering before its behavior matters.

## Editing the world outside the tick pass

Anything that writes `WorldChunks` directly — player editing, the falling
block entity, furnaces changing their lit state — bypasses the hooks, so it
must report the write:

```rust
let metadata = chunks.metadata_at(x, y, z);
if let Some(previous) = chunks.set_block(x, y, z, block) {
    ticks.block_changed(IVec3::new(x, y, z), previous, metadata);
}
```

The next tick pass runs the old block's `on_removed`, the new block's
`on_added`, and notifies the neighbors, as `setBlockWithNotify` would have.
Interactions go through `BlockEvent::{Clicked, Activated, Walked,
Harvested}`. Streaming and persistence still need their own
`request_block_update` / `mark_dirty` calls for such writes; changes made
inside the tick pass get them automatically.

## Effects

Behaviors never touch the ECS. `drop_block_as_item` and
`spawn_falling_block` queue a `TickEffect`, which `apply_tick_effects`
turns into entities after the pass. A new kind of spawned entity or
presentation effect (particles, sounds) adds a `TickEffect` variant, a
`TickWorld` method that queues it, and a match arm there.

## Persistence

Metadata saves with each chunk. Scheduled ticks are saved per chunk with
their remaining delay: when a chunk unloads its ticks move into
`Chunk::pending_ticks` and are written with it, a loaded chunk that is
saved writes a copy of its pending ticks, and loading schedules them
again. Beta dropped such ticks,
which could leave water frozen mid-flow; this is a deliberate difference.

Block ticks mark their chunk dirty on every change, so flowing water can
make most of a region dirty between autosaves. The save path handles that
load: an autosave drains a few chunks per frame and writes them on a
background task, so a churning world does not stall. A chunk whose snapshot
is already with the writer and which then unloads is queued again, because
streaming attaches its dropped items and pending ticks at unload time.

## Generated springs

Overworld `WorldGenLiquids` attempts run after cactus and before snow, with
Beta's population RNG. They place flowing water or lava, notify neighbors,
then directly update the source with immediate scheduled updates. Later pending
ticks travel with their chunks. Immediate spread only sees population's four loaded chunks; nested fluid draws use a reproducibly seeded world RNG
instead of Java's runtime-seeded `World.rand`.

## Not implemented yet

- **Weather.** Nothing is rained on, so farmland only hydrates from water
  and snow never accumulates. Put rain in `TickWorld::rained_on` and the
  snowfall half of `freeze_column`.
- **Redstone details.** Dust, torches (with Beta's burnout), repeaters,
  levers, buttons, pressure plates, doors, trapdoors, pistons, rails, note
  blocks, TNT, and dispensers have update behaviors
  (`behaviors/{redstone,controls,piston,rail,note,tnt,dispenser,fixtures}.rs`).
  Power is queried through `BlockBehavior::{can_provide_power, weak_power,
  strong_power}` and the `TickWorld::block_*_powered` helpers, which mirror
  `World.isBlockIndirectlyGettingPowered` and friends. What relays power
  and holds dust, torches, and rails is `Block::is_normal_cube`, which is
  not the mesher's opaque-cube flag: pistons and TNT are excluded and mob
  spawners included. Pistons move blocks
  immediately and push bodies out of the new head with
  `TickEffect::PistonPush`, rather than animating `TileEntityPiston`. A
  piston notified while any piston is mid-move schedules itself for the
  next tick instead of acting, standing in for `ignoreUpdates`. Rails take
  their shape from `RailLogic` when placed and keep it afterward, as in
  Beta; only a three-way junction beside a power source switches.
  Dispensers persist nine slots and fire arrows (as `entity::projectiles`
  arrows) or eject items; eggs and snowballs are ejected as items. Note
  blocks keep their pitch but make no sound until audio is implemented.
  Plates and detector rails read a per-tick snapshot of nearby bodies
  (`BlockTicks::set_occupants`); arrows do not trigger wooden plates yet.
  Minecarts (`entity::minecart`) follow rails and are saved with their chunk;
  chest and furnace carts do not exist.
- Presentation-only hooks: `randomDisplayTick`, redstone ore sparkles, lava
  fizz and smoke, and fluid sounds.
