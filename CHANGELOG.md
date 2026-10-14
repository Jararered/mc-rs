# Changelog

High-level, player-visible changes, written as things to test in game. Each release has the same four sections
(Blocks, Items, Mobs, Generation), each split into Additions, Interactions and Fixes. Additions are grouped under a
heading for the item, block or mob they belong to. Tick a box once it has been checked in play.

Every entry ends with the commit that made the change and when it was committed (`hash · date time`, local time,
from `git log --date=format:'%Y-%m-%d %H:%M'`). A commit cannot name its own hash, so an entry is written as
`94d9f67 · 2026-10-09 08:53` in the commit that makes the change and gets its hash and time the next time this file is edited.

## Unreleased

### Blocks

#### Additions

##### Sign

- [ ] Right-click with a sign on top of a solid block stands a post turned to face you; on the side of one it
      hangs a board. It cannot go on the underside of a block or on glass. A post is drawn square to the
      nearer axis, not at its sixteenth of a turn.
      `de86ed1 · 2026-10-10 07:44`
- [ ] A sign pops off as an item when the block under a post or behind a board is taken away.
      `de86ed1 · 2026-10-10 07:44`
- [ ] A sign keeps its four lines through a save in either format, and one in a Beta world loads with its text.
      Nothing draws the text or lets you write it in this game yet: on the server, a Beta client writes a new
      sign once and every client reads it.
      `de86ed1 · 2026-10-10 07:44`

##### Sound

Sounds need the original game's `resources/` folder beside `assets/`. Without it the game starts and stays silent.

- [ ] Walking is heard every block and two thirds, by what is underfoot: stone, wood, grass, gravel (dirt too),
      sand and wool each sound different. A snow layer muffles the block under it, sneaking is silent, and
      swimming makes no footsteps.
      `f813674 · 2026-10-12 05:55`
- [ ] Holding the button on a block taps it five times a second. Breaking it plays its sound louder (glass
      shatters, sand breaks like gravel, iron and gold blocks ring higher), and so does placing one.
      `f813674 · 2026-10-12 05:55`
- [ ] Levers, buttons and pressure plates click, higher going on than coming off. Pistons thump out and back. A
      door or trapdoor creaks open or shut, by hand or by redstone. A dispenser twangs when it shoots, clicks
      when it drops an item, and gives a higher click when empty.
      `f813674 · 2026-10-12 05:55`
- [ ] A powered or right-clicked note block plays its note: harp on most blocks, bass on wood, snare on sand or
      gravel, hat on glass, bass drum on stone, an octave either side of the middle.
      `f813674 · 2026-10-12 05:55`
- [ ] TNT hisses as its fuse is lit, and an explosion booms and is heard from as far as 64 blocks.
      `f813674 · 2026-10-12 05:55`
- [ ] Fire crackles, flowing water runs, a portal hums, lava that meets water hisses, and a redstone torch that
      burns out fizzes.
      `f813674 · 2026-10-12 05:55`
- [ ] Rain patters around you, quieter and duller under a roof, and thunder follows every lightning strike
      however far off.
      `f813674 · 2026-10-12 05:55`
- [ ] Somewhere dark with no sky above (a cave, or the Nether), an eerie noise plays from nearby now and then,
      never within five minutes of the last.
      `f813674 · 2026-10-12 05:55`
- [ ] A sound fades evenly with distance and is gone 16 blocks away, and it comes from the side it is on: turn
      around and it changes ears.
      `f813674 · 2026-10-12 05:55`

##### Jukebox

- [ ] Right-click a jukebox with a music disc (13 or cat) to put it in: the disc leaves your hand, its track
      plays from the jukebox, and "Now playing: C418 - cat" shows over the hotbar for three seconds, changing
      color and fading out.
      `9959e9e · 2026-10-12 06:54`
- [ ] Right-click a playing jukebox, with anything in hand, and the disc pops out of the top and the music
      stops. Breaking the jukebox does the same.
      `9959e9e · 2026-10-12 06:54`
- [ ] A record is heard up to 64 blocks away, quieter with distance and from the side the jukebox is on. Starting
      one stops the background music, none starts while it plays, and a second jukebox takes over from the
      first. Its volume is the Jukebox and note blocks slider.
      `9959e9e · 2026-10-12 06:54`
- [ ] A jukebox keeps its disc through a save and reload in either world format (it does not start playing
      again by itself), and a Beta client on the server hears discs that other players start.
      `9959e9e · 2026-10-12 06:54`

##### Cake

- [ ] Right-click a solid block with a cake to set it down. It will not sit on air, a torch, or snow, and it will
      not go where you are standing. It is a low block, and each bite comes off its west side.
      `7181961 · 2026-10-14 18:26`
- [ ] Right-click the cake while hurt: one of six slices is eaten and you heal three half-hearts, up to full.
      Punching it only digs. At full health a right-click takes no bite and does not use whatever is in your hand.
      `7181961 · 2026-10-14 18:26`
- [ ] A dropped or thrown cake is the flat cake picture, as in your hand, not a small cake block.
      `7181961 · 2026-10-14 18:26`
- [ ] The sixth bite removes the cake. Breaking one, or taking away the block under it, leaves nothing behind.
      `7181961 · 2026-10-14 18:26`

#### Interactions

- [ ] A powered dispenser launches eggs and snowballs as projectiles instead of dropping them as items.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] An arrow fired by a dispenser can be picked up once it has stuck in a block.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] An arrow, snowball, egg or fishing bobber resting on a wooden pressure plate presses it; a stone plate ignores
      them.
      `a22f8d3 · 2026-10-09 07:55`

#### Fixes

- [ ] Three stone, sandstone, planks or cobblestone in a row craft three slabs of that material. A full grid of
      nine no longer does.
      `48841f5 · 2026-10-11 08:39`
- [ ] A fence is two rows of three sticks and sandstone is four sand in a square, not a full grid of nine.
      `48841f5 · 2026-10-11 08:39`
- [ ] A slab resting on a slab of another material shows its underside from below.
      `48841f5 · 2026-10-11 08:39`
- [ ] The side of a block next to a slab, stair or farmland is lit, not black, and a slab's top under a ceiling
      is lit from the open sides.
      `48841f5 · 2026-10-11 08:39`
- [ ] Crops, grass and saplings read the light of a slab, stair or farmland cell as the brightest cell beside or
      above it.
      `48841f5 · 2026-10-11 08:39`
- [ ] A slab, glass, a fence or any other block you would collide with cannot be placed where you are standing.
      A slab can still go under you when only its upper half would overlap.
      `48841f5 · 2026-10-11 08:39`
- [ ] Looking through two or more water surfaces (a waterfall in front of a lake, or the surface from below while
      swimming) shows the far one behind the near one, and the order holds as you walk around.
      `2fc1df7 · 2026-10-14 14:06`
- [ ] Chunks hidden behind terrain are no longer drawn: in a cave, or behind a hill, nothing that should be
      visible goes missing, even turning quickly, walking through a tunnel, or breaking into a cave from above.
      `2fc1df7 · 2026-10-14 14:06`

### Items

#### Additions

##### Bow

- [ ] Right-click shoots one arrow taken from anywhere in the inventory; nothing happens with no arrows. Holding
      the button keeps shooting.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] The arm does not swing when the bow is used, with or without Bow charging.
      `f1418cc · 2026-10-09 08:18`
- [ ] Features, Bow charging (off by default): hold right-click to draw, release to shoot. A short tap shoots
      nothing and keeps the arrow; a full draw (about a second) shoots fastest and hits hardest. Walking is slowed
      and the bow pulls back in first person while drawing. Switching slot or opening the inventory cancels the draw.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] Bow charging: the view narrows gradually as the bow is drawn (up to 15% at full draw) and eases back to
      normal after the shot or a cancelled draw.
      `f1418cc · 2026-10-09 08:18`

##### Arrow

- [ ] Arrows you shot can be walked over and picked up after they stick and stop quivering. Skeleton arrows cannot.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] A picked-up arrow flies to you like a dropped item rather than vanishing.
      `f1418cc · 2026-10-09 08:18`

##### Snowball and egg

- [ ] Both are thrown on right-click and use up one from the stack.
      `a22f8d3 · 2026-10-09 07:55`

##### Fishing rod

- [ ] Right-click casts a bobber, right-click again reels it in. The bobber floats on water, the line is drawn to
      the hand, and the held rod shows its cast icon while the line is out.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] Wait for the bobber to dip, then reel in for a raw fish thrown toward you. Bites come sooner in rain.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] Rod wear: 1 per fish, 2 when the bobber was stuck in a block, 3 when it had hooked a mob, 0 for an empty reel.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] The line drops if you switch away from the rod or move more than 32 blocks from the bobber.
      `a22f8d3 · 2026-10-09 07:55`

##### Boat

- [ ] Right-click with a boat places it on the block or the water you are looking at, up to five blocks away; one
      set on a snow layer rests on the block beneath.
      `94d9f67 · 2026-10-09 08:53`
- [ ] Features, Steady boats (on by default): a boat dropped on water bobs for a second or two and then sits still
      at the waterline. Off, it keeps bobbing a few centimetres for ever, as in Beta.
      `95b1313 · 2026-10-09 09:07`
- [ ] Features, Boat crashes (on by default): running a boat into a block at speed breaks it, head-on or at an
      angle; a gentle bump only stops it. Off, only scraping along a wall or shore breaks it, as in Beta.
      `95b1313 · 2026-10-09 09:07`
- [ ] A boat floats at the surface of water, rises when pushed under, and falls and grinds to a halt on land.
      `94d9f67 · 2026-10-09 08:53`
- [ ] Right-click a boat to sit in it and right-click it again to step out. W, A, S and D push it the way you
      look, sneaking pushes more gently, and the boat turns to trail its wake.
      `94d9f67 · 2026-10-09 08:53`
- [ ] A wrecked boat breaks into three planks and two sticks and puts its rider off.
      `94d9f67 · 2026-10-09 08:53`
- [ ] Five quick punches break a boat into the same planks and sticks; it rocks with each hit and the damage wears
      off if you stop.
      `94d9f67 · 2026-10-09 08:53`
- [ ] Walking into a boat pushes it, two boats push each other apart, and a boat clears the snow layers under it.
      `94d9f67 · 2026-10-09 08:53`
- [ ] A boat is still there, with its heading, after saving and reloading, in both save formats.
      `94d9f67 · 2026-10-09 08:53`

##### Painting

- [ ] Right-click the side of a block with a painting to hang one there, facing you. It picks a random motive
      among those that fit: a lone block takes a one-block picture, and a wide wall can take the big ones, up to
      four blocks across. It will not go on the top or bottom of a block, on a non-solid block such as a torch,
      with a block in front of it, or over another painting.
      `pending`
- [ ] Each motive shows its picture from `art/kz.png`, with a wooden back and frame, lit block by block. Without
      `assets/` a painting is a plain brown panel.
      `pending`
- [ ] Take the wall away and within five seconds the painting drops as an item. Any hit, or an explosion nearby,
      knocks it off the wall the same way.
      `pending`
- [ ] A painting stays on its wall with the same motive after saving and reloading, in both save formats, and one
      in a Beta world loads.
      `pending`
- [ ] On the server, a Beta client sees paintings and can punch them down.
      `pending`

##### Settings

- [ ] F3 shows the frame rate on its first line, as `N fps`, counted afresh each second.
      `8b1d6d5 · 2026-10-14 14:17`
- [ ] Video, Max FPS dragged all the way right reads Unlimited and no longer caps the frame rate. A new VSync
      button (on by default) holds frames to the display; with it off and Max FPS at Unlimited, the frame rate
      climbs past the refresh rate. Both survive a restart.
      `8b1d6d5 · 2026-10-14 14:17`
- [ ] Video, Anti-aliasing starts OFF in a new settings file, as Beta has none; a settings file that already has it
      keeps its choice. With it off the frame rate is higher, most of all on a high-resolution display.
      `8723961 · 2026-10-14 15:01`
- [ ] The sky, sun, moon, stars and sunrise glow look as before, by day, at dusk, at night and in rain: terrain,
      clouds and mobs cover them, water and rain are drawn over them, and the Nether shows its fog color. The frame
      rate is higher, most of all while turning at a long render distance.
      `8723961 · 2026-10-14 15:01`
- [ ] Features, Cake by right-click only (on by default): punching a cake digs it without taking a bite. Off, a
      punch eats a slice too, as in Beta.
      `7181961 · 2026-10-14 18:26`
- [ ] Features, Pig steering (on by default): a ridden pig turns to where you look and walks while W is held,
      hopping up blocks in its way. Off, the pig wanders wherever it likes, as in Beta.
      `94d9f67 · 2026-10-09 08:53`
- [ ] Settings has a new Features tab for deliberate departures from Beta. Both options survive a restart.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] The settings tabs read Gameplay, Controls, Video, Features.
      `d2fb059 · 2026-10-09 08:06`

##### Sound and music

- [ ] Settings has an Audio tab with a slider from OFF to 100% for Master volume, Music, Blocks, Weather, Hostile
      mobs, Passive mobs, Players, Ambient, and Jukebox and note blocks. Master scales everything, music included; each of
      the others scales its own kind under it. All are remembered.
      `ba41405 · 2026-10-12 06:13`
- [ ] A slider changes what is already playing, and a kind at OFF starts no new sound: with Hostile mobs at OFF
      a zombie is silent while a pig beside it is not. Music or Master at OFF stops the track.
      `ba41405 · 2026-10-12 06:13`
- [ ] Blocks covers footsteps, digging, placing, switches, pistons, doors, fuses and explosions; Players covers
      your hurt grunt, the bow, throws, pickups and splashes; Ambient covers cave noises, fire, running water
      and portals. A menu button's click follows Master alone.
      `f813674 · 2026-10-12 05:55`
- [ ] Music plays in a world: the first track within ten minutes, then ten to twenty minutes of quiet after each,
      from the twelve in `resources/music` and `resources/newmusic`.
      `f813674 · 2026-10-12 05:55`
- [ ] Every menu button clicks.
      `f813674 · 2026-10-12 05:55`
- [ ] The bow twangs, and a snowball, egg or cast line leaves the hand with a softer, lower one. An arrow
      thunks into what it hits. With Bow Charging on, a fuller draw sounds higher.
      `f813674 · 2026-10-12 05:55`
- [ ] Picking up an item or a spent arrow pops. An item that falls in lava fizzes as it is spat out.
      `f813674 · 2026-10-12 05:55`
- [ ] Flint and steel strikes, a hoe sounds like the soil it turns, a water bucket emptied in the Nether hisses,
      and a bobber splashes when a fish bites.
      `f813674 · 2026-10-12 05:55`
- [ ] Standing in a portal gives a rising rush as it starts to charge and another as it takes you, which plays
      on through the loading screen.
      `f813674 · 2026-10-12 05:55`

#### Interactions

- [ ] Shift-clicking an item out of a chest puts it in the hotbar first, then the rest of the inventory.
      `6c8444b · 2026-10-10 01:52`
- [ ] Using a bow, snowball, egg or rod while pointing at a block does not place or activate anything extra, but
      doors, levers, chests and other blocks that react to a right-click still take the click first.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] A thrown item, arrow or bobber does not hit you as it leaves your hand.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] `/clear` empties your whole inventory: hotbar, main slots, armor, the 2×2 crafting grid and anything held
      on the cursor. Chat reports how many items went.
      `3966173 · 2026-10-11 03:31`

#### Fixes

- [ ] Each dye shows its own icon: a bone crafts into white bone meal, not what looked like yellow dye, and ink
      sacs, lapis and cocoa beans look like themselves.
      `a722019 · 2026-10-11 09:06`
- [ ] Bone meal used on a sapling grows its tree at once (the bone meal is spent even when there is no room), on
      wheat ripens it, and on a grass block sprouts tall grass with the odd dandelion and rose around it.
      `2505096 · 2026-10-11 09:10`
- [ ] A dye and white wool craft wool of that dye's color (bone meal leaves it white, an ink sac makes black).
      `a722019 · 2026-10-11 09:06`
- [ ] Dyes mix: rose red and bone meal make pink, rose red and dandelion yellow make orange, and the rest of
      Beta's mixes (lime, gray, light gray, light blue, cyan, purple, magenta) work.
      `a722019 · 2026-10-11 09:06`
- [ ] View bobbing: the view and the held item or arm swing as far and as fast as Beta's when walking, the same
      at any frame rate, instead of a slower, smaller sway. The swing does not advance while sneaking.
      `cb5dc12 · 2026-10-11 03:19`
- [ ] Holding sneak dips the view a little at once, and it eases back up on release. Blocks are still picked
      where the crosshair points.
      `cb5dc12 · 2026-10-11 03:19`
- [ ] Arrows (yours, skeletons' and dispensers') no longer flicker where their faces overlap.
      `f1418cc · 2026-10-09 08:18`
- [ ] Dropped items fly smoothly to the player when picked up, including while walking or sprinting into them,
      instead of stuttering.
      `f1418cc · 2026-10-09 08:18`
- [ ] Features, Floating items (on by default): dropped items in water rise and bob at the surface instead of
      sinking, and still drift with flowing water. Turning it off restores Beta's sinking.
      `a22f8d3 · 2026-10-09 07:55`

### Mobs

#### Additions

##### Pig

- [ ] Right-click a saddled pig to ride it and right-click it again to get off. An unsaddled pig cannot be ridden.
      `94d9f67 · 2026-10-09 08:53`
- [ ] You sit on the pig's back and are carried wherever it goes; a pig that dies or unloads lets you go.
      `94d9f67 · 2026-10-09 08:53`
- [ ] A pig that falls with you on it hurts you as much as the fall hurts the pig.
      `94d9f67 · 2026-10-09 08:53`

##### Chicken

- [ ] A thrown egg has a one in eight chance to hatch a chicken where it lands, and rarely hatches four.
      `a22f8d3 · 2026-10-09 07:55`

##### Voices

- [ ] Mobs speak now and then: pigs, sheep, chickens, cows (quietly), zombies, skeletons, spiders and zombie
      pigmen, and a ghast's moan carries a long way. Creepers, slimes and squid have no idle sound, and animals
      speak less often than monsters.
      `f813674 · 2026-10-12 05:55`
- [ ] A mob cries out when hit and makes its death sound with the blow that kills it. You grunt when hurt.
      `f813674 · 2026-10-12 05:55`
- [ ] A creeper hisses as it starts to swell. A skeleton's bow twangs. A ghast shrieks as it charges and again
      as it fires. A zombie pigman snarls a moment after it turns on you.
      `f813674 · 2026-10-12 05:55`
- [ ] A slime squelches as it hops (not the smallest) and as it lands (only the biggest), and slaps when it
      hurts you. A chicken plops out its egg.
      `f813674 · 2026-10-12 05:55`
- [ ] A wolf barks, sometimes pants, whines when tame and badly hurt, growls when angry, and rattles as it
      shakes itself dry.
      `f813674 · 2026-10-12 05:55`
- [ ] Anything that drops into water splashes, louder the faster it went in; a burning mob or player that
      reaches water or rain hisses out; and a fall that hurts lands with a thud of the block below.
      `f813674 · 2026-10-12 05:55`

#### Interactions

- [ ] Your arrows damage mobs (4 with the Beta bow) and make hostile mobs turn on you.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] A snowball or egg knocks a mob back and makes it flinch without hurting it.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] A bobber that touches a mob hooks it and rides on it; reeling in drags the mob toward you.
      `a22f8d3 · 2026-10-09 07:55`

#### Fixes

- [ ] In creative or spectator mode, hostile mobs no longer walk up to you, slimes no longer hop after you, and
      mobs do not turn to look at you. Back in survival they hunt you again.
      `2316921 · 2026-10-11 03:26`
- [ ] The hearts and air bubbles are hidden in creative and spectator mode and return with `/gamemode survival`.
      `2316921 · 2026-10-11 03:26`

### Generation

#### Additions

- Nothing this round.

#### Interactions

- Nothing this round.

#### Fixes

- Nothing this round.
