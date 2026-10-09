mod boat;
mod combat;
mod dropped_items;
pub(crate) mod mobs;

pub(crate) mod pathfinding;
mod projectiles;

use bevy::prelude::*;
use game::entity::CollisionState;
use game::entity::EntitySize;
use game::entity::Gravity;
use game::entity::StepHeight;
use game::entity::Velocity;
use game::player::Player;

#[test]
fn player_inserts_physics_components() {
    let mut world = World::new();
    let id = world
        .spawn((Player, Transform::from_xyz(1.0, 2.0, 3.0)))
        .id();
    let entity = world.entity(id);
    assert!(entity.contains::<Velocity>());
    assert!(entity.contains::<CollisionState>());
    assert!(entity.contains::<Gravity>());
    assert_eq!(
        entity.get::<EntitySize>().copied(),
        Some(EntitySize::PLAYER)
    );
    assert_eq!(
        entity.get::<StepHeight>().copied(),
        Some(StepHeight::PLAYER)
    );
    assert_eq!(
        entity.get::<Transform>().unwrap().translation,
        Vec3::new(1.0, 2.0, 3.0)
    );
}
