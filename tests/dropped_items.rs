use bevy::prelude::Vec3;
use game::entity::EntitySize;
use game::entity::dropped_items::block_drop;
use game::entity::dropped_items::item_bob_offset;
use game::item::ItemStack;
use game::world::block::block::BlockId;

#[test]
fn block_drop_uses_beta_stone_rule() {
    assert_eq!(
        block_drop(BlockId::Stone),
        Some(ItemStack::from_block(BlockId::Cobblestone, 1).unwrap())
    );
    assert_eq!(
        block_drop(BlockId::Dirt),
        Some(ItemStack::from_block(BlockId::Dirt, 1).unwrap())
    );
}

#[test]
fn block_drop_preserves_wood_and_leaf_metadata() {
    assert_eq!(block_drop(BlockId::SpruceWood).unwrap().data(), 1);
    assert_eq!(block_drop(BlockId::BirchWood).unwrap().data(), 2);
    assert_eq!(block_drop(BlockId::SpruceLeaves).unwrap().data(), 1);
    assert_eq!(block_drop(BlockId::BirchLeaves).unwrap().data(), 2);
    assert_eq!(block_drop(BlockId::TorchWest).unwrap().data(), 0);
}

#[test]
fn dropped_item_collision_box_is_small_and_centered() {
    let aabb = EntitySize::DROPPED_ITEM.aabb(Vec3::new(2.5, 3.0, 4.5));
    assert_eq!(aabb.min, Vec3::new(2.375, 3.0, 4.375));
    assert_eq!(aabb.max, Vec3::new(2.625, 3.25, 4.625));
}

#[test]
fn item_bob_is_a_visual_offset_around_the_collision_feet() {
    let feet = Vec3::new(2.5, 65.0, 4.5);
    let aabb = EntitySize::DROPPED_ITEM.aabb(feet);
    assert!((aabb.min.y - feet.y).abs() < 1e-5);
    assert!((item_bob_offset(0.0, 0.0)).abs() < 1e-5);
    assert!((item_bob_offset(std::f32::consts::FRAC_PI_2, 0.0) - 0.04).abs() < 1e-5);
    assert!((EntitySize::DROPPED_ITEM.aabb(feet).min.y - feet.y).abs() < 1e-5);
}
