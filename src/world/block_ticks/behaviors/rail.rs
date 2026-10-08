//! Beta's normal, powered and detector rails.
use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;
use crate::world::block_ticks::behavior;
use bevy::math::IVec3;

pub struct Rail;
pub static RAIL: Rail = Rail;
pub struct Detector;
pub static DETECTOR: Detector = Detector;

fn is_rail(block: Block) -> bool {
    matches!(
        block,
        Block::Rail | Block::PoweredRail | Block::DetectorRail
    )
}

fn rail_at(world: &TickWorld, cell: IVec3) -> bool {
    is_rail(world.block(cell))
}

/// Beta's `RailLogic`: one rail and the cells its shape joins. A rail only
/// takes a new neighbor while it has a free end, so finished track keeps its
/// shape when another rail is laid beside it.
struct RailLogic {
    position: IVec3,
    /// Powered and detector rails: straight only, with the power bit above
    /// the shape.
    straight_only: bool,
    connected: Vec<IVec3>,
}

impl RailLogic {
    fn new(world: &TickWorld, position: IVec3) -> Self {
        let straight_only = world.block(position) != Block::Rail;
        let mut shape = world.metadata(position);
        if straight_only {
            shape &= 7;
        }
        let mut logic = Self {
            position,
            straight_only,
            connected: Vec::with_capacity(2),
        };
        logic.set_connections(shape);
        logic
    }

    fn set_connections(&mut self, shape: u8) {
        let (north, south) = (IVec3::NEG_Z, IVec3::Z);
        let (west, east) = (IVec3::NEG_X, IVec3::X);
        let up = IVec3::Y;
        let ends = match shape {
            0 => [north, south],
            1 => [west, east],
            2 => [west, east + up],
            3 => [west + up, east],
            4 => [north + up, south],
            5 => [north, south + up],
            6 => [east, south],
            7 => [west, south],
            8 => [west, north],
            9 => [east, north],
            _ => {
                self.connected.clear();
                return;
            }
        };
        self.connected.clear();
        self.connected
            .extend(ends.map(|offset| self.position + offset));
    }

    /// The rail at `cell`, or one step above or below it.
    fn logic_near(world: &TickWorld, cell: IVec3) -> Option<Self> {
        [IVec3::ZERO, IVec3::Y, IVec3::NEG_Y]
            .into_iter()
            .map(|step| cell + step)
            .find(|&cell| rail_at(world, cell))
            .map(|cell| Self::new(world, cell))
    }

    fn is_track_near(world: &TickWorld, cell: IVec3) -> bool {
        rail_at(world, cell) || rail_at(world, cell + IVec3::Y) || rail_at(world, cell - IVec3::Y)
    }

    /// `func_785_b`: keep only the ends that hold a rail joined back to this
    /// one, at the height that rail really is.
    fn prune(&mut self, world: &TickWorld) {
        let mut kept = Vec::with_capacity(2);
        for &cell in &self.connected {
            if let Some(other) = Self::logic_near(world, cell)
                && other.joins(self.position)
            {
                kept.push(other.position);
            }
        }
        self.connected = kept;
    }

    /// `isConnectedTo` and `isInTrack`: columns are compared, not heights.
    fn joins(&self, cell: IVec3) -> bool {
        self.connected
            .iter()
            .any(|end| end.x == cell.x && end.z == cell.z)
    }

    /// `getAdjacentTracks`.
    fn adjacent_tracks(&self, world: &TickWorld) -> usize {
        [IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X]
            .into_iter()
            .filter(|&offset| Self::is_track_near(world, self.position + offset))
            .count()
    }

    /// `handleKeyPress`: joined already, or an end is free.
    fn accepts(&self, other: IVec3) -> bool {
        self.joins(other) || self.connected.len() != 2
    }

    /// `func_786_c`: whether the rail toward `cell` would join this one.
    fn can_join(&self, world: &TickWorld, cell: IVec3) -> bool {
        Self::logic_near(world, cell).is_some_and(|mut other| {
            other.prune(world);
            other.accepts(self.position)
        })
    }

    /// The sloped form of a straight shape with a rail one step up.
    fn sloped(&self, world: &TickWorld, shape: u8) -> u8 {
        let above = self.position + IVec3::Y;
        let mut shape = shape;
        if shape == 0 {
            if rail_at(world, above + IVec3::NEG_Z) {
                shape = 4;
            }
            if rail_at(world, above + IVec3::Z) {
                shape = 5;
            }
        } else if shape == 1 {
            if rail_at(world, above + IVec3::X) {
                shape = 2;
            }
            if rail_at(world, above + IVec3::NEG_X) {
                shape = 3;
            }
        }
        shape
    }

    fn write_shape(&self, world: &mut TickWorld, shape: u8) {
        let metadata = if self.straight_only {
            world.metadata(self.position) & 8 | shape
        } else {
            shape
        };
        world.set_metadata_notify(self.position, metadata);
    }

    /// `func_788_d`: take `other` as an end and reshape around the result.
    fn join(&mut self, world: &mut TickWorld, other: IVec3) {
        self.connected.push(other);
        let north = self.joins(self.position + IVec3::NEG_Z);
        let south = self.joins(self.position + IVec3::Z);
        let west = self.joins(self.position + IVec3::NEG_X);
        let east = self.joins(self.position + IVec3::X);
        let mut shape = 0;
        if west || east {
            shape = 1;
        }
        if !self.straight_only {
            if south && east && !north && !west {
                shape = 6;
            }
            if south && west && !north && !east {
                shape = 7;
            }
            if north && west && !south && !east {
                shape = 8;
            }
            if north && east && !south && !west {
                shape = 9;
            }
        }
        let shape = self.sloped(world, shape);
        self.write_shape(world, shape);
    }

    /// `refreshTrackShape`: pick the shape that joins the neighbors with a
    /// free end. With three or four of them a plain rail is a switch, and
    /// `powered` picks which curve it takes.
    fn refresh(&mut self, world: &mut TickWorld, powered: bool, force: bool) {
        let north = self.can_join(world, self.position + IVec3::NEG_Z);
        let south = self.can_join(world, self.position + IVec3::Z);
        let west = self.can_join(world, self.position + IVec3::NEG_X);
        let east = self.can_join(world, self.position + IVec3::X);
        let mut shape = None;
        if (north || south) && !west && !east {
            shape = Some(0);
        }
        if (west || east) && !north && !south {
            shape = Some(1);
        }
        if !self.straight_only {
            if south && east && !north && !west {
                shape = Some(6);
            }
            if south && west && !north && !east {
                shape = Some(7);
            }
            if north && west && !south && !east {
                shape = Some(8);
            }
            if north && east && !south && !west {
                shape = Some(9);
            }
        }
        let shape = shape.unwrap_or_else(|| {
            let mut shape = 0;
            if west || east {
                shape = 1;
            }
            if !self.straight_only {
                let corners = if powered {
                    [
                        (south && east, 6),
                        (west && south, 7),
                        (east && north, 9),
                        (north && west, 8),
                    ]
                } else {
                    [
                        (north && west, 8),
                        (east && north, 9),
                        (west && south, 7),
                        (south && east, 6),
                    ]
                };
                for (exists, corner) in corners {
                    if exists {
                        shape = corner;
                    }
                }
            }
            shape
        });
        // With no neighbor to join, Beta skips the slope check too.
        let shape = if north || south || west || east {
            self.sloped(world, shape)
        } else {
            shape
        };
        self.set_connections(shape);
        let metadata = if self.straight_only {
            world.metadata(self.position) & 8 | shape
        } else {
            shape
        };
        if force || world.metadata(self.position) != metadata {
            world.set_metadata_notify(self.position, metadata);
            for cell in self.connected.clone() {
                if let Some(mut other) = Self::logic_near(world, cell) {
                    other.prune(world);
                    if other.accepts(self.position) {
                        other.join(world, self.position);
                    }
                }
            }
        }
    }
}

fn refresh_shape(world: &mut TickWorld, position: IVec3, force: bool) {
    let powered = world.block_indirectly_getting_powered(position);
    RailLogic::new(world, position).refresh(world, powered, force);
}

fn propagation(world: &mut TickWorld, pos: IVec3, forward: bool, depth: u8) -> bool {
    if depth >= 8 {
        return false;
    }
    let shape = world.metadata(pos) & 7;
    let (offset, may_descend) = match (shape, forward) {
        (0, true) => (IVec3::Z, true),
        (0, false) => (IVec3::NEG_Z, true),
        (1, true) => (IVec3::NEG_X, true),
        (1, false) => (IVec3::X, true),
        (2, true) => (IVec3::NEG_X, true),
        (2, false) => (IVec3::X + IVec3::Y, false),
        (3, true) => (IVec3::NEG_X + IVec3::Y, false),
        (3, false) => (IVec3::X, true),
        (4, true) => (IVec3::Z, true),
        (4, false) => (IVec3::NEG_Z + IVec3::Y, false),
        (5, true) => (IVec3::Z + IVec3::Y, false),
        (5, false) => (IVec3::NEG_Z, true),
        _ => return false,
    };
    let axis_z = matches!(shape, 0 | 4 | 5);
    for next in [
        Some(pos + offset),
        may_descend.then_some(pos + offset - IVec3::Y),
    ]
    .into_iter()
    .flatten()
    {
        if world.block(next) != Block::PoweredRail {
            continue;
        }
        let next_meta = world.metadata(next);
        if next_meta & 8 != 0 && matches!(next_meta & 7, 0 | 4 | 5) == axis_z {
            if world.block_indirectly_getting_powered(next)
                || world.block_indirectly_getting_powered(next + IVec3::Y)
                || propagation(world, next, forward, depth + 1)
            {
                return true;
            }
        }
    }
    false
}
fn update_power(world: &mut TickWorld, position: IVec3) {
    let old = world.metadata(position);
    let powered = world.block_indirectly_getting_powered(position)
        || world.block_indirectly_getting_powered(position + IVec3::Y)
        || propagation(world, position, true, 0)
        || propagation(world, position, false, 0);
    if powered != (old & 8 != 0) {
        world.set_metadata_notify(position, (old & 7) | if powered { 8 } else { 0 });
        world.notify_neighbors(position - IVec3::Y, Block::PoweredRail);
        if matches!(old & 7, 2..=5) {
            world.notify_neighbors(position + IVec3::Y, Block::PoweredRail);
        }
    }
}
impl BlockBehavior for Rail {
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        refresh_shape(world, position, true);
        // Beta leaves a new powered rail dark until a neighbor changes; it
        // is lit straight away here.
        if world.block(position) == Block::PoweredRail {
            update_power(world, position);
        }
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, neighbor: Block) {
        let block = world.block(position);
        let meta = world.metadata(position);
        let shape = if block == Block::Rail { meta } else { meta & 7 };
        let slope_support = match shape {
            2 => Some(IVec3::X),
            3 => Some(IVec3::NEG_X),
            4 => Some(IVec3::NEG_Z),
            5 => Some(IVec3::Z),
            _ => None,
        };
        if !world.is_normal_cube(position - IVec3::Y)
            || slope_support.is_some_and(|dir| !world.is_normal_cube(position + dir))
        {
            world.drop_block_as_item(position, block, meta);
            world.set_block_notify(position, Block::Air);
        } else if block == Block::PoweredRail {
            update_power(world, position);
        } else if block == Block::Rail
            && behavior(neighbor).can_provide_power()
            && RailLogic::new(world, position).adjacent_tracks(world) == 3
        {
            // A switch: the junction follows the power beside it.
            refresh_shape(world, position, false);
        }
    }
}
impl BlockBehavior for Detector {
    fn can_provide_power(&self) -> bool {
        true
    }
    fn ticks_randomly(&self, _: Block) -> bool {
        true
    }
    fn tick_rate(&self, _: Block) -> u32 {
        20
    }
    fn weak_power(&self, world: &mut TickWorld, position: IVec3, _: u8) -> bool {
        world.metadata(position) & 8 != 0
    }
    fn strong_power(&self, world: &mut TickWorld, position: IVec3, side: u8) -> bool {
        side == 1 && self.weak_power(world, position, side)
    }
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        RAIL.on_added(world, position);
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, neighbor: Block) {
        RAIL.neighbor_changed(world, position, neighbor);
    }
    fn entity_collided(&self, world: &mut TickWorld, position: IVec3) {
        if world.metadata(position) & 8 == 0 {
            update_detector(world, position);
        }
    }
    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.metadata(position) & 8 != 0 {
            update_detector(world, position);
        }
    }
}
fn update_detector(world: &mut TickWorld, pos: IVec3) {
    let meta = world.metadata(pos);
    let occupied = world.occupant_on(pos, Block::DetectorRail);
    if occupied != (meta & 8 != 0) {
        world.set_metadata_notify(pos, (meta & 7) | if occupied { 8 } else { 0 });
        world.notify_neighbors(pos, Block::DetectorRail);
        world.notify_neighbors(pos - IVec3::Y, Block::DetectorRail);
    }
    if occupied {
        world.schedule(pos, Block::DetectorRail, 20);
    }
}
