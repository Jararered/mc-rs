# Multiplayer TODO

What is left before multiplayer is finished, and what is already in place. The design and the rules for
working on it are in `AGENTS.md` under "Players and multiplayer"; this file is only the list. Tick an item
when it lands and add what you find missing.

## Done

- [x] Simulation handles several players in one world: mob targeting, spawning, random ticks, lightning,
      pickups, damage, hazards, the all-asleep night skip (`Player` / `LocalPlayer`)
- [x] Streaming loads around every player (`streaming::Viewers`)
- [x] Both dimensions at once, one headless world each (`world::host::WorldHost`), with players moved
      between them through the teleporter
- [x] Saving per dimension through one storage, and named player records in both save formats
- [x] A player's hands as messages: dig, break, use, use entity, drop (`PlayerAction`), with containers
      opening by `WindowOpen`
- [x] Item use, beds and chat commands act for the player who sent them
- [x] Dedicated server binary (`cargo run --bin server`) on the shared `game` library
- [x] Beta 1.7.3 protocol: login, chunks with light, movement, digging and placing, block changes, chat,
      inventory window, health, time, keep-alive, other players (spawn, move, swing, leave)

## Verify first

- [ ] Connect the original Beta 1.7.3 client to the server. Nothing below matters until this works; so far
      only a test client has spoken to it.
- [ ] Two original clients at once: see each other, dig and place near each other, chat
- [ ] Walk far enough to load and unload chunks; leave and rejoin at the same place
- [ ] Go through a Nether portal and back on the server
- [ ] Play singleplayer through the changed paths: mining and instant breaks, placing, buckets, doors, beds,
      all container types, boats, carts, bow, rod, chat commands

## Beta server: what clients are not told

- [ ] Mobs: `Packet24MobSpawn`, entity metadata, relative moves and teleports, velocity, status (hurt, death)
- [ ] Dropped items: `Packet21PickupSpawn`, `Packet22Collect`
- [ ] Vehicles and projectiles: `Packet23VehicleSpawn` for boats, carts, arrows, snowballs, eggs, bobbers,
      falling blocks, primed TNT; `Packet39AttachEntity` for riding
- [ ] Turn the Peaceful default off once mobs are visible (`ServerConfig::peaceful`, `--monsters`)
- [ ] `Packet7UseEntity`: attack and interact, which needs the entity ids above
- [ ] Another player's held item and armor changing (`Packet5PlayerInventory`), sneaking, being hurt
- [ ] Inventory clicks: `Packet102WindowClick` answered with `Packet106Transaction` instead of undone
- [ ] Container windows: `Packet100OpenWindow`, `Packet104WindowItems`, `Packet105UpdateProgressbar` for
      chests, furnaces, workbenches, dispensers
- [ ] Signs (`Packet130UpdateSign`), note blocks (`Packet54`), doors and sounds (`Packet61DoorChange`)
- [ ] Weather (`Packet71Weather`, `Packet70Bed`), sleeping (`Packet17Sleep`), explosions (`Packet60Explosion`)
- [ ] Many block changes in one chunk as `Packet52MultiBlockChange`
- [ ] Respawn after death: check the flow against the real client

## Beta server: trust and robustness

- [ ] Dig time check, as `ItemInWorldManager` does, instead of trusting the client's status 2
- [ ] Reach and line-of-sight checks on dig, place and use
- [ ] Movement checks: speed, "moved wrongly", illegal stance
- [ ] Fall damage for remote players (nothing integrates them on the server)
- [ ] Username check against a session server, or an explicit offline-mode setting
- [ ] Time-out for a client that stops sending, and a cap on connections
- [ ] Command permissions (ops), so not every player can `/give` and `/tp`
- [ ] Chat and command feedback to the right audience: chat to everyone, feedback to the sender only
      (one `ChatHistory` per dimension today)
- [ ] Logging to the terminal (join, leave, errors); the server sets up no logger
- [ ] Chunk compression and sending off the main thread
- [ ] Stop on a signal as well as on Enter
- [ ] `server.properties`-style configuration (spawn protection, max players, MOTD)

## Simulation: still single-player

- [ ] Container state per player: `InventorySession`, `ActiveWorkbench` and `WorldChunks::open_cart` are
      one-per-world, and the inventory screen edits inventories, chests and furnaces directly
- [ ] Movement, look and the selected slot as messages rather than writes to the local player's components
- [ ] The Bow Charging draw for a player other than the local one
- [ ] World rules out of `GameSettings`: difficulty and the Features toggles (floating items, pig steering,
      steady boats, boat crashes, bow charging) belong to the world, not a client's settings file
- [ ] A tamed wolf follows its owner, not the nearest player; `Mob::owner` is a fixed string
- [ ] A picked-up item and a collected arrow fly to the player who took them
- [ ] Mob attacks set their target to the attacker everywhere a mob is angered, not only on a direct hit
- [ ] A remote player who dies where they cannot respawn (Nether, bed in unloaded chunks) is sent to the
      Overworld by the host, as the local player is by the session

## Server architecture

- [ ] Light without meshes, so a hosted dimension stops building meshes nobody draws and the server no
      longer needs the rendering plugin's streaming wiring
- [ ] Split generation (around every player) from meshing (around the local player) in streaming
- [ ] Teleporter off the host's thread: it reads up to 289 chunks and may generate terrain synchronously
- [ ] A rendering-free core crate (workspace split), so the server does not link Bevy's renderer
- [ ] `WorldHost` join: wait for the spawn chunk before placing a new player (the Beta server does this
      itself today)
- [ ] Benchmark the server tick with several players spread apart

## Our own client

- [ ] Draw other players: a model, name tags, held item, arm swing, hurt flash
- [ ] Our own protocol and transport, Beta-shaped; the server sends raw blocks and the client lights and
      meshes them
- [ ] A remote-world mode for the client: no local generation, block ticks, mobs or saving
- [ ] The game on a local server: singleplayer through the same client/server path
- [ ] Join screen: address entry, connecting and disconnected notices
- [ ] Host a world from the game for others to join
- [ ] Optionally, a Beta protocol client so the game can join original Beta servers
