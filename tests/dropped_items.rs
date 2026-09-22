use bevy::mesh::Mesh;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Vec3;
use game::entity::EntitySize;
use game::entity::dropped_items::block_drop;
use game::entity::dropped_items::block_drop_position;
use game::entity::dropped_items::hotbar_icon_scale;
use game::entity::dropped_items::item_bob_offset;
use game::entity::dropped_items::item_constructor_motion;
use game::entity::dropped_items::item_motion_after_collision;
use game::entity::dropped_items::item_piece_transform;
use game::entity::dropped_items::item_pile_offsets;
use game::entity::dropped_items::item_reaches_player;
use game::entity::dropped_items::item_slipperiness;
use game::entity::dropped_items::item_spin_yaw;
use game::entity::dropped_items::item_stack_copies;
use game::entity::dropped_items::item_visual_yaw;
use game::entity::dropped_items::pickup_position;
use game::entity::dropped_items::thrown_item_motion;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::inventory::MAIN_SLOTS;
use game::item::ItemStack;
use game::world::block::block::BlockId;
use game::world::meshing::dropped_block_meshes;

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
fn dropped_item_collision_box_is_centered_on_the_transform() {
    let center = Vec3::new(2.5, 3.0, 4.5);
    let aabb = EntitySize::DROPPED_ITEM.aabb(center);
    assert_eq!(aabb.min, Vec3::new(2.375, 2.875, 4.375));
    assert_eq!(aabb.max, Vec3::new(2.625, 3.125, 4.625));
    assert!((aabb.min.y - (center.y - 0.125)).abs() < 1e-5);
}

#[test]
fn item_bob_floats_above_the_collision_center() {
    let center = Vec3::new(2.5, 65.125, 4.5);
    let aabb = EntitySize::DROPPED_ITEM.aabb(center);
    assert!((aabb.min.y - 65.0).abs() < 1e-5);
    assert!((item_bob_offset(0.0, 0.0, 0.0) - 0.1).abs() < 1e-5);
    assert!((item_bob_offset(0.0, 0.0, std::f32::consts::FRAC_PI_2) - 0.2).abs() < 1e-5);
    assert!((EntitySize::DROPPED_ITEM.aabb(center).min.y - 65.0).abs() < 1e-5);
}

#[test]
fn item_drag_matches_entity_item() {
    let air = item_motion_after_collision(Vec3::splat(1.0), false, false, false, false, 0.6);
    assert!((air.x - 0.98).abs() < 1e-5);
    assert!((air.y - 0.98).abs() < 1e-5);
    assert!((air.z - 0.98).abs() < 1e-5);

    let ground =
        item_motion_after_collision(Vec3::new(0.1, -0.04, 0.0), false, true, false, true, 0.6);
    assert!((ground.x - 0.1 * 0.6 * 0.98).abs() < 1e-5);
    assert_eq!(ground.y, 0.0);

    let ice = item_motion_after_collision(
        Vec3::new(1.0, 0.0, -1.0),
        false,
        true,
        false,
        true,
        item_slipperiness(Some(BlockId::Ice)),
    );
    assert!((ice.x - 0.98 * 0.98).abs() < 1e-5);
    assert!((ice.z + 0.98 * 0.98).abs() < 1e-5);
    assert_eq!(ice.y, 0.0);

    let mut slide = Vec3::new(0.1, 0.0, 0.0);
    for _ in 0..20 {
        slide = item_motion_after_collision(slide, false, true, false, true, 0.6);
    }
    assert!((slide.x - 0.1 * 0.588_f32.powi(20)).abs() < 1e-6);
    assert_eq!(slide.y, 0.0);
}

#[test]
fn block_drop_position_stays_inside_the_cell() {
    let origin = bevy::prelude::IVec3::new(3, 64, -2);
    let low = block_drop_position(origin, Vec3::ZERO);
    let high = block_drop_position(origin, Vec3::ONE);
    assert!((low.x - 3.15).abs() < 1e-5);
    assert!((low.y - 64.15).abs() < 1e-5);
    assert!((low.z - (-1.85)).abs() < 1e-5);
    assert!((high.x - 3.85).abs() < 1e-5);
    assert!((high.y - 64.85).abs() < 1e-5);
    assert!((high.z - (-1.15)).abs() < 1e-5);
}

#[test]
fn constructor_and_throw_motion_use_tick_units() {
    let spawned = item_constructor_motion(0.0, 1.0);
    assert!((spawned.x + 0.1).abs() < 1e-5);
    assert!((spawned.y - 0.2).abs() < 1e-5);
    assert!((spawned.z - 0.1).abs() < 1e-5);

    let thrown = thrown_item_motion(Vec3::NEG_Z, 0.0, 0.0, 0.0, 0.0);
    assert!(thrown.x.abs() < 1e-5);
    assert!((thrown.y - 0.1).abs() < 1e-5);
    assert!((thrown.z + 0.3).abs() < 1e-5);
}

#[test]
fn cubes_spin_on_y_and_sprites_only_face_the_camera() {
    assert!((item_spin_yaw(20.0, 0.0, 0.5) - 1.5).abs() < 1e-5);
    assert!((item_visual_yaw(true, 1.25, 0.4) - 1.25).abs() < 1e-5);
    assert!((item_visual_yaw(false, 1.25, 0.4) - 0.4).abs() < 1e-5);
    let pose = item_piece_transform(0.2, 0.4, 0.5, Vec3::ZERO);
    let (yaw, pitch, roll) = pose.rotation.to_euler(bevy::prelude::EulerRot::YXZ);
    assert!((yaw - 0.4).abs() < 1e-5);
    assert!(pitch.abs() < 1e-5 && roll.abs() < 1e-5);
    assert!((pose.translation.y - 0.2).abs() < 1e-5);
}

#[test]
fn stack_copies_follow_beta_thresholds() {
    assert_eq!(item_stack_copies(1), 1);
    assert_eq!(item_stack_copies(2), 2);
    assert_eq!(item_stack_copies(6), 3);
    assert_eq!(item_stack_copies(21), 4);
    let cube = item_pile_offsets(2, true, 0.25);
    let sprite = item_pile_offsets(2, false, 0.5);
    assert_eq!(cube[0], Vec3::ZERO);
    assert_eq!(sprite[0], Vec3::ZERO);
    assert!(cube[1].abs().max_element() <= 0.8 + 1e-4);
    assert!(sprite[1].abs().max_element() <= 0.3 + 1e-4);
    assert_ne!(cube[1], Vec3::ZERO);
    assert_eq!(
        item_pile_offsets(3, true, 0.25),
        item_pile_offsets(3, true, 0.25)
    );
}

#[test]
fn dropped_blocks_use_the_world_cube() {
    let dirt = dropped_block_meshes(BlockId::Dirt, true, [0.2, 0.8, 0.3], [1.0, 1.0, 1.0]);
    assert_eq!(position_count(&dirt.body), 24);
    assert!(dirt.overlay.is_none());
    assert!(!dirt.cutout);

    let grass = dropped_block_meshes(BlockId::Grass, true, [0.2, 0.8, 0.3], [1.0, 1.0, 1.0]);
    assert_eq!(position_count(grass.overlay.as_ref().unwrap()), 16);
    let fast = dropped_block_meshes(BlockId::Grass, false, [0.2, 0.8, 0.3], [1.0, 1.0, 1.0]);
    assert!(fast.overlay.is_none());

    let positions = positions_of(&grass.body);
    assert!(positions.iter().all(|position| position[0].abs() <= 0.51));
    let uvs = uvs_of(&grass.body);
    assert_ne!(uvs[0], uvs[8]);

    let colors = colors_of(&dirt.body);
    assert!((colors[0][0] - 1.0).abs() < 1e-5);
    assert!((colors[4][0] - 0.55).abs() < 1e-5);

    let leaves = dropped_block_meshes(BlockId::Leaves, true, [1.0; 3], [0.2, 0.7, 0.1]);
    assert!(leaves.cutout);
    assert!(!dropped_block_meshes(BlockId::Leaves, false, [1.0; 3], [0.2, 0.7, 0.1]).cutout);
}

fn position_count(mesh: &Mesh) -> usize {
    positions_of(mesh).len()
}

fn positions_of(mesh: &Mesh) -> &[[f32; 3]] {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(values)) => values,
        _ => panic!("mesh positions"),
    }
}

fn uvs_of(mesh: &Mesh) -> &[[f32; 2]] {
    match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(values)) => values,
        _ => panic!("mesh uvs"),
    }
}

#[test]
fn pickup_uses_the_expanded_player_box() {
    let eye = Vec3::new(8.0, 66.62, 8.0);
    let at_feet = Vec3::new(8.0, 65.125, 8.0);
    let beside = Vec3::new(9.2, 65.125, 8.0);
    let too_far = Vec3::new(10.0, 65.125, 8.0);
    assert!(item_reaches_player(EntitySize::PLAYER, eye, at_feet));
    assert!(item_reaches_player(EntitySize::PLAYER, eye, beside));
    assert!(!item_reaches_player(EntitySize::PLAYER, eye, too_far));
}

#[test]
fn a_full_inventory_keeps_the_whole_stack() {
    let mut hotbar = Hotbar::default();
    let mut inventory = Inventory::default();
    let full = ItemStack::from_block(BlockId::Dirt, 64).unwrap();
    hotbar.slots = [Some(full); 9];
    inventory.main = [Some(full); MAIN_SLOTS];
    let incoming = ItemStack::from_block(BlockId::Dirt, 3).unwrap();
    let remainder = inventory.insert(&mut hotbar, incoming).unwrap();
    assert_eq!(remainder.count(), incoming.count());
    assert!(hotbar.pop.iter().all(|pop| *pop == 0));
}

#[test]
fn a_partial_fit_pops_the_hotbar_slot_and_returns_the_rest() {
    let mut hotbar = Hotbar::default();
    let mut inventory = Inventory::default();
    let full = ItemStack::from_block(BlockId::Dirt, 64).unwrap();
    hotbar.slots = [Some(full); 9];
    hotbar.slots[3] = Some(ItemStack::from_block(BlockId::Dirt, 63).unwrap());
    inventory.main = [Some(full); MAIN_SLOTS];
    let remainder = inventory
        .insert(
            &mut hotbar,
            ItemStack::from_block(BlockId::Dirt, 5).unwrap(),
        )
        .unwrap();
    assert_eq!(remainder.count(), 4);
    assert_eq!(hotbar.slots[3].unwrap().count(), 64);
    assert_eq!(hotbar.pop[3], 5);
    assert!(
        hotbar
            .pop
            .iter()
            .enumerate()
            .all(|(index, pop)| index == 3 || *pop == 0)
    );
}

#[test]
fn pickup_flight_eases_toward_the_chest() {
    let start = Vec3::new(0.0, 0.0, 0.0);
    let eye = Vec3::new(0.0, 2.0, 0.0);
    let mid = pickup_position(start, eye, 1.0, 0.5);
    assert!((mid.y - 1.5 * 0.25).abs() < 1e-5);
    assert!((mid.x).abs() < 1e-5);
    let done = pickup_position(start, eye, 3.0, 0.0);
    assert!((done.y - 1.5).abs() < 1e-5);
}

#[test]
fn hotbar_pop_matches_the_beta_slot_scale() {
    let scale = hotbar_icon_scale(5, 0.0);
    assert!((scale.x - 0.5).abs() < 1e-5);
    assert!((scale.y - 1.5).abs() < 1e-5);
    assert_eq!(hotbar_icon_scale(0, 0.4), bevy::prelude::Vec2::ONE);
}

#[test]
fn take_selected_drops_one_item() {
    let mut hotbar = Hotbar::default();
    assert!(hotbar.take_selected(1).is_none());
    hotbar.slots[0] = Some(ItemStack::from_block(BlockId::Cobblestone, 2).unwrap());
    let taken = hotbar.take_selected(1).unwrap();
    assert_eq!(taken.count(), 1);
    assert_eq!(hotbar.selected_stack().unwrap().count(), 1);
    let last = hotbar.take_selected(1).unwrap();
    assert_eq!(last.count(), 1);
    assert!(hotbar.selected_stack().is_none());
}

fn colors_of(mesh: &Mesh) -> &[[f32; 4]] {
    match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(values)) => values,
        _ => panic!("mesh colors"),
    }
}
