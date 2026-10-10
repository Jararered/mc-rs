//! Placing blocks: Beta's `ItemBlock.onItemUse` checks and each block's
//! facing or shape on placement.

use crate::block::bed;
use crate::block::blocks::Block;
use crate::block::direction::Direction;
use crate::block::properties::cactus_can_stay;
use crate::block::properties::plant_ground_can_hold;
use crate::block::properties::sugar_cane_can_stay;
use crate::physics::Aabb;
use crate::physics::BlockFace;
use crate::physics::BlockHit;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::WorldChunks;
use bevy::prelude::*;

/// Attach a torch to the hit face. Fails without a solid support block.
pub fn place_block(chunks: &mut WorldChunks, hit: BlockHit, player: Aabb) -> bool {
    place_selected_block(chunks, hit, player, Block::Torch)
}

pub fn place_selected_block(
    chunks: &mut WorldChunks,
    hit: BlockHit,
    player: Aabb,
    selected: Block,
) -> bool {
    place_selected_block_facing(chunks, hit, player, selected, 0, Direction::South)
}

pub fn place_selected_block_facing(
    chunks: &mut WorldChunks,
    hit: BlockHit,
    player: Aabb,
    selected: Block,
    species: u8,
    front: Direction,
) -> bool {
    let (x, y, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
    if y < 0 || y >= CHUNK_HEIGHT as i32 {
        return false;
    }
    let Some(current) = chunks.block_at(x, y, z) else {
        return false;
    };
    if !current.is_replaceable() {
        return false;
    }
    if selected == Block::Chest && !chest_can_place_at(chunks, x, y, z) {
        return false;
    }
    if selected == Block::Ladder && ladder_facing(chunks, x, y, z, hit.face, hit.block).is_none() {
        return false;
    }
    if selected.is_crossed_plant()
        && selected != Block::SugarCane
        && !chunks
            .block_at(x, y - 1, z)
            .is_some_and(|ground| plant_ground_can_hold(selected, ground))
    {
        return false;
    }
    if selected == Block::SugarCane {
        let Some(below) = chunks.block_at(x, y - 1, z) else {
            return false;
        };
        let adjacent_water = [
            chunks.block_at(x - 1, y - 1, z),
            chunks.block_at(x + 1, y - 1, z),
            chunks.block_at(x, y - 1, z - 1),
            chunks.block_at(x, y - 1, z + 1),
        ]
        .map(|block| matches!(block, Some(Block::Water | Block::FlowingWater)));
        if !sugar_cane_can_stay(below, adjacent_water) {
            return false;
        }
    }
    if selected == Block::Cactus {
        let Some(below) = chunks.block_at(x, y - 1, z) else {
            return false;
        };
        let [Some(west), Some(east), Some(north), Some(south)] = [
            chunks.block_at(x - 1, y, z),
            chunks.block_at(x + 1, y, z),
            chunks.block_at(x, y, z - 1),
            chunks.block_at(x, y, z + 1),
        ] else {
            return false;
        };
        if !cactus_can_stay(below, [west, east, north, south]) {
            return false;
        }
    }
    if selected == Block::Pumpkin
        && !chunks
            .block_at(x, y - 1, z)
            .is_some_and(Block::is_opaque_cube)
    {
        return false;
    }
    if !hit.block.is_opaque_cube() && selected == Block::Torch {
        return false;
    }
    // `canPlaceBlockAt`: dust, repeaters, plates, and rails sit on a full cube.
    if matches!(
        selected,
        Block::RedstoneWire
            | Block::Repeater
            | Block::StonePressurePlate
            | Block::WoodenPressurePlate
            | Block::Rail
            | Block::PoweredRail
            | Block::DetectorRail
    ) && !chunks
        .block_at(x, y - 1, z)
        .is_some_and(Block::is_normal_cube)
    {
        return false;
    }
    // Levers, buttons, and redstone torches hang on the side or top of a cube.
    if matches!(
        selected,
        Block::Lever | Block::StoneButton | Block::RedstoneTorch
    ) && (!hit.block.is_normal_cube()
        || hit.face == BlockFace::Down
        || (selected == Block::StoneButton && hit.face == BlockFace::Up))
    {
        return false;
    }
    // `BlockFence.canPlaceBlockAt`: on another fence or on solid ground.
    if selected == Block::Fence
        && !chunks
            .block_at(x, y - 1, z)
            .is_some_and(|below| below == Block::Fence || below.is_solid_material())
    {
        return false;
    }
    let (block, metadata) = match selected {
        Block::Torch => {
            let support = match hit.face {
                BlockFace::Up => None,
                BlockFace::Down => return false,
                BlockFace::West => Some(Direction::East),
                BlockFace::East => Some(Direction::West),
                BlockFace::North => Some(Direction::South),
                BlockFace::South => Some(Direction::North),
            };
            (
                selected,
                support.map_or(0, |facing| selected.facing_metadata(facing)),
            )
        }
        Block::Furnace | Block::Pumpkin | Block::Chest | Block::Dispenser => {
            (selected, selected.facing_metadata(front))
        }
        // `BlockStairs.onBlockPlacedBy`: the steps climb away from the player.
        Block::WoodenStairs | Block::CobblestoneStairs => (
            selected,
            match front {
                Direction::North => 2,
                Direction::East => 1,
                Direction::South => 3,
                Direction::West => 0,
            },
        ),
        // `BlockTrapDoor.canPlaceBlockOnSide` and `onBlockPlaced`: hinged on
        // the side of a full cube.
        Block::Trapdoor => {
            if !hit.block.is_normal_cube() {
                return false;
            }
            let metadata = match hit.face {
                BlockFace::North => 0,
                BlockFace::South => 1,
                BlockFace::West => 2,
                BlockFace::East => 3,
                BlockFace::Up | BlockFace::Down => return false,
            };
            (selected, metadata)
        }
        Block::Ladder => {
            let Some(support) = ladder_facing(chunks, x, y, z, hit.face, hit.block) else {
                return false;
            };
            (selected, selected.facing_metadata(support))
        }
        // Beta's `onBlockPlaced` names the clicked side: 5 is the floor.
        Block::Lever | Block::StoneButton | Block::RedstoneTorch => (
            selected,
            match hit.face {
                BlockFace::West => 2,
                BlockFace::East => 1,
                BlockFace::North => 4,
                BlockFace::South => 3,
                BlockFace::Up | BlockFace::Down => 5,
            },
        ),
        // `BlockRedstoneRepeater.onBlockPlacedBy`: the repeater points away
        // from the player and takes its input from behind.
        Block::Repeater => (
            selected,
            match front {
                Direction::South => 0,
                Direction::West => 1,
                Direction::North => 2,
                Direction::East => 3,
            },
        ),
        Block::Piston | Block::StickyPiston => {
            (selected, piston_placement_facing(player, (x, y, z), front))
        }
        _ => (selected, species),
    };
    if block.is_opaque_cube()
        && player.intersects(Aabb::new(
            Vec3::new(x as f32, y as f32, z as f32),
            Vec3::new(x as f32 + 1.0, y as f32 + 1.0, z as f32 + 1.0),
        ))
    {
        return false;
    }
    chunks
        .set_block_with_metadata(x, y, z, block, metadata)
        .is_some_and(|(previous, _)| previous != block)
}

/// `ItemDoor.onItemUse`: stand a two-block door on the top face of a full
/// cube. `front` is the side facing the player, as furnaces take it. The
/// hinge goes to the side with more solid blocks, or beside another door so
/// the pair opens from the middle.
pub fn place_door(chunks: &mut WorldChunks, hit: BlockHit, door: Block, front: Direction) -> bool {
    if hit.face != BlockFace::Up {
        return false;
    }
    let (x, y, z) = (hit.x, hit.y + 1, hit.z);
    // `BlockDoor.canPlaceBlockAt`.
    let free = |chunks: &WorldChunks, y: i32| {
        chunks
            .block_at(x, y, z)
            .is_some_and(|block| block.is_replaceable())
    };
    if y >= CHUNK_HEIGHT as i32 - 1
        || !hit.block.is_normal_cube()
        || !free(chunks, y)
        || !free(chunks, y + 1)
    {
        return false;
    }
    // Beta's `(yaw + 180) * 4 / 360 - 0.5` quadrant.
    let mut facing: u8 = match front {
        Direction::North => 1,
        Direction::East => 2,
        Direction::South => 3,
        Direction::West => 0,
    };
    let (dx, dz) = match facing {
        0 => (0, 1),
        1 => (-1, 0),
        2 => (0, -1),
        _ => (1, 0),
    };
    let cubes = |chunks: &WorldChunks, sx: i32, sz: i32| {
        (0..2)
            .filter(|dy| {
                chunks
                    .block_at(sx, y + dy, sz)
                    .is_some_and(Block::is_opaque_cube)
            })
            .count()
    };
    let has_door = |chunks: &WorldChunks, sx: i32, sz: i32| {
        (0..2).any(|dy| chunks.block_at(sx, y + dy, sz) == Some(door))
    };
    let behind = cubes(chunks, x - dx, z - dz);
    let ahead = cubes(chunks, x + dx, z + dz);
    let door_behind = has_door(chunks, x - dx, z - dz);
    let door_ahead = has_door(chunks, x + dx, z + dz);
    if (door_behind && !door_ahead) || ahead > behind {
        facing = (facing.wrapping_sub(1) & 3) + 4;
    }
    chunks.set_block_with_metadata(x, y, z, door, facing);
    chunks.set_block_with_metadata(x, y + 1, z, door, facing + 8);
    true
}

/// `ItemSign.onItemUse`: a post on top of a solid block, turned to face the
/// player in sixteenths of a circle, or a board on its side. Returns the
/// sign's cell and what was there.
pub fn place_sign(
    chunks: &mut WorldChunks,
    hit: BlockHit,
    look: Vec3,
) -> Option<(IVec3, Block, u8)> {
    if hit.face == BlockFace::Down || !hit.block.is_solid_material() {
        return None;
    }
    let (x, y, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
    let previous = chunks.block_at(x, y, z)?;
    if !previous.is_replaceable() {
        return None;
    }
    let previous_metadata = chunks.metadata_at(x, y, z);
    let (block, metadata) = match hit.face {
        BlockFace::Up => {
            // Beta's yaw: 0 looks along +Z and 90 along -X.
            let yaw = (-look.x).atan2(look.z).to_degrees();
            let turn = ((yaw + 180.0) * 16.0 / 360.0 + 0.5).floor() as i32 & 15;
            (Block::StandingSign, turn as u8)
        }
        BlockFace::North => (Block::WallSign, 2),
        BlockFace::South => (Block::WallSign, 3),
        BlockFace::West => (Block::WallSign, 4),
        _ => (Block::WallSign, 5),
    };
    chunks.set_block_with_metadata(x, y, z, block, metadata);
    Some((IVec3::new(x, y, z), previous, previous_metadata))
}

/// `BlockPistonBase.determineOrientation`: close to the placed block, the
/// player's eye height takes priority over horizontal facing. The collision
/// box starts at the feet, so `min.y + 1.82` is Beta's placement height
/// (`posY + 1.82 - yOffset`).
pub(super) fn piston_placement_facing(
    player: Aabb,
    (x, y, z): (i32, i32, i32),
    front: Direction,
) -> u8 {
    let player_x = (player.min.x + player.max.x) * 0.5;
    let player_z = (player.min.z + player.max.z) * 0.5;
    if (player_x - x as f32).abs() < 2.0 && (player_z - z as f32).abs() < 2.0 {
        let placement_height = player.min.y + 1.82;
        if placement_height - y as f32 > 2.0 {
            return 1;
        }
        if y as f32 - placement_height > 0.0 {
            return 0;
        }
    }
    match front {
        Direction::North => 2,
        Direction::East => 5,
        Direction::South => 3,
        Direction::West => 4,
    }
}

pub(super) fn ladder_facing(
    chunks: &WorldChunks,
    x: i32,
    y: i32,
    z: i32,
    hit_face: BlockFace,
    hit_block: Block,
) -> Option<Direction> {
    let support_at = |facing: Direction| {
        let [dx, dy, dz] = facing.offset();
        chunks
            .block_at(x + dx, y + dy, z + dz)
            .filter(|block| (*block).is_opaque_cube())
            .map(|_| facing)
    };

    let clicked_wall = match hit_face {
        BlockFace::West if hit_block.is_opaque_cube() => Some(Direction::East),
        BlockFace::East if hit_block.is_opaque_cube() => Some(Direction::West),
        BlockFace::North if hit_block.is_opaque_cube() => Some(Direction::South),
        BlockFace::South if hit_block.is_opaque_cube() => Some(Direction::North),
        _ => None,
    };
    clicked_wall
        .and_then(support_at)
        // Beta's onBlockPlaced fallback checks +Z, -Z, +X, -X.
        .or_else(|| support_at(Direction::South))
        .or_else(|| support_at(Direction::North))
        .or_else(|| support_at(Direction::East))
        .or_else(|| support_at(Direction::West))
}

pub(super) fn chest_can_place_at(chunks: &WorldChunks, x: i32, y: i32, z: i32) -> bool {
    let neighbors = [(x - 1, y, z), (x + 1, y, z), (x, y, z - 1), (x, y, z + 1)];
    let chests = neighbors
        .into_iter()
        .filter(|&(nx, ny, nz)| chunks.block_at(nx, ny, nz).is_some_and(Block::is_chest))
        .collect::<Vec<_>>();
    match chests.as_slice() {
        [] => true,
        [neighbor] => chunks
            .chest_group_at(neighbor.0, neighbor.1, neighbor.2)
            .is_some_and(|group| !group.is_double()),
        _ => false,
    }
}

/// `ItemBed.onItemUse`: lay a bed on the top face of a normal cube, pointing
/// away from the player. `front` is the side facing the player, as furnaces
/// take it. Returns the two cells written, the clicked one first.
pub fn place_bed(chunks: &mut WorldChunks, hit: BlockHit, front: Direction) -> Option<[IVec3; 2]> {
    if hit.face != BlockFace::Up {
        return None;
    }
    // Beta's `yaw * 4 / 360 + 0.5` quadrant: the way the player looks.
    let direction: u8 = match front {
        Direction::North => 0,
        Direction::East => 1,
        Direction::South => 2,
        Direction::West => 3,
    };
    let near = IVec3::new(hit.x, hit.y + 1, hit.z);
    let far = near + bed::head_to_foot(direction);
    let fits = |chunks: &WorldChunks, cell: IVec3| {
        chunks.block_at(cell.x, cell.y, cell.z) == Some(Block::Air)
            && chunks
                .block_at(cell.x, cell.y - 1, cell.z)
                .is_some_and(Block::is_normal_cube)
    };
    if !fits(chunks, near) || !fits(chunks, far) {
        return None;
    }
    chunks.set_block_with_metadata(near.x, near.y, near.z, Block::Bed, direction);
    chunks.set_block_with_metadata(far.x, far.y, far.z, Block::Bed, direction + bed::FOOT);
    Some([near, far])
}

pub(super) fn furnace_facing_toward_player(player_forward: Vec3) -> Direction {
    let toward_player = Vec2::new(-player_forward.x, -player_forward.z);
    if toward_player.x.abs() > toward_player.y.abs() {
        if toward_player.x >= 0.0 {
            Direction::East
        } else {
            Direction::West
        }
    } else if toward_player.y >= 0.0 {
        Direction::South
    } else {
        Direction::North
    }
}
