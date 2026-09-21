//! World item entities used when a container closes with no inventory space.

use bevy::prelude::*;

use crate::app::state::AppScreen;
use crate::entity::DroppedItem;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::player::Player;

pub struct DroppedItemPlugin;

impl Plugin for DroppedItemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            pickup_dropped_items.run_if(in_state(AppScreen::Playing)),
        );
    }
}

/// Try to merge nearby dropped stacks into the player's inventory. The item
/// remains in the world if the inventory is still full.
fn pickup_dropped_items(
    mut commands: Commands,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>,
    mut items: Query<(Entity, &Transform, &mut DroppedItem)>,
) {
    let Ok((player_transform, mut hotbar, mut inventory)) = player.single_mut() else {
        return;
    };
    for (entity, transform, mut dropped) in &mut items {
        if transform
            .translation
            .distance_squared(player_transform.translation)
            > 2.25
        {
            continue;
        }
        if let Some(remainder) = inventory.insert(&mut hotbar, dropped.0) {
            dropped.0 = remainder;
        } else {
            commands.entity(entity).despawn();
        }
    }
}
