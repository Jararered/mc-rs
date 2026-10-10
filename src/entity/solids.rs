//! Entity solid boxes that [`move_entity`](crate::physics::move_entity) can
//! stand on. Boats expose theirs via Beta's `EntityBoat.getBoundingBox`.

use bevy::prelude::*;

use crate::entity::boat::BOAT_SIZE;
use crate::entity::boat::Boat;
use crate::physics::Aabb;
use crate::player::Player;
use crate::world::block_ticks::BlockTickSet;

/// Boat AABBs refreshed each frame before creature and player movement.
#[derive(Resource, Default, Debug)]
pub struct BoatSolids(pub Vec<Aabb>);

pub struct BoatSolidsPlugin;

impl Plugin for BoatSolidsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BoatSolids>().add_systems(
            Update,
            refresh_boat_solids
                .before(BlockTickSet)
                .before(crate::physics::PhysicsSet::Integrate),
        );
    }
}

fn refresh_boat_solids(
    mut solids: ResMut<BoatSolids>,
    boats: Query<&Transform, (With<Boat>, Without<Player>)>,
) {
    solids.0.clear();
    solids.0.extend(
        boats
            .iter()
            .map(|transform| BOAT_SIZE.aabb(transform.translation)),
    );
}
