# Changelog

High-level, player-visible changes, written as things to test in game. Each release has the same four sections
(Blocks, Items, Mobs, Generation), each split into Additions, Interactions and Fixes. Additions are grouped under a
heading for the item, block or mob they belong to. Tick a box once it has been checked in play.

Every entry ends with the commit that made the change and when it was committed (`hash · date time`, local time,
from `git log --date=format:'%Y-%m-%d %H:%M'`). A commit cannot name its own hash, so an entry is written as
`pending` in the commit that makes the change and gets its hash and time the next time this file is edited.

## Unreleased

### Blocks

#### Additions

- Nothing this round.

#### Interactions

- [ ] A powered dispenser launches eggs and snowballs as projectiles instead of dropping them as items.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] An arrow fired by a dispenser can be picked up once it has stuck in a block.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] An arrow, snowball, egg or fishing bobber resting on a wooden pressure plate presses it; a stone plate ignores
      them.
      `a22f8d3 · 2026-10-09 07:55`

#### Fixes

- Nothing this round.

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

##### Settings

- [ ] Settings has a new Features tab for deliberate departures from Beta. Both options survive a restart.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] The settings tabs read Gameplay, Controls, Video, Features.
      `d2fb059 · 2026-10-09 08:06`

#### Interactions

- [ ] Using a bow, snowball, egg or rod while pointing at a block does not place or activate anything extra, but
      doors, levers, chests and other blocks that react to a right-click still take the click first.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] A thrown item, arrow or bobber does not hit you as it leaves your hand.
      `a22f8d3 · 2026-10-09 07:55`

#### Fixes

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

##### Chicken

- [ ] A thrown egg has a one in eight chance to hatch a chicken where it lands, and rarely hatches four.
      `a22f8d3 · 2026-10-09 07:55`

#### Interactions

- [ ] Your arrows damage mobs (4 with the Beta bow) and make hostile mobs turn on you.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] A snowball or egg knocks a mob back and makes it flinch without hurting it.
      `a22f8d3 · 2026-10-09 07:55`
- [ ] A bobber that touches a mob hooks it and rides on it; reeling in drags the mob toward you.
      `a22f8d3 · 2026-10-09 07:55`

#### Fixes

- Nothing this round.

### Generation

#### Additions

- Nothing this round.

#### Interactions

- Nothing this round.

#### Fixes

- Nothing this round.
