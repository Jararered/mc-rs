use bevy::prelude::Vec3;
use game::block::blocks::Block;
use game::block::direction::Direction;
use game::entity::EntitySize;
use game::entity::drops::blocks::DropRoll;
use game::entity::drops::blocks::natural_drops;
use game::entity::drops::blocks::natural_drops_with_metadata;
use game::entity::drops::blocks::player_break_drops_with_metadata;
use game::entity::drops::items::block_drop_position;
use game::entity::drops::items::dropped_block_model;
use game::entity::drops::items::hotbar_icon_scale;
use game::entity::drops::items::interpolated_item_position;
use game::entity::drops::items::item_bob_offset;
use game::entity::drops::items::item_constructor_motion;
use game::entity::drops::items::item_motion_after_collision;
use game::entity::drops::items::item_piece_transform;
use game::entity::drops::items::item_pile_offsets;
use game::entity::drops::items::item_reaches_player;
use game::entity::drops::items::item_slipperiness;
use game::entity::drops::items::item_spin_yaw;
use game::entity::drops::items::item_stack_copies;
use game::entity::drops::items::item_visual_yaw;
use game::entity::drops::items::pickup_position;
use game::entity::drops::items::thrown_item_motion;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::inventory::MAIN_SLOTS;
use game::item::Item;
use game::item::ItemStack;
use game::rendering::meshing::BlockGeometry;
use game::rendering::meshing::BlockLighting;
use game::rendering::meshing::dropped_block_meshes;

struct Rolls<'a> {
    values: &'a [u32],
    index: usize,
}

impl DropRoll for Rolls<'_> {
    fn next_int(&mut self, bound: u32) -> u32 {
        let value = self.values[self.index];
        self.index += 1;
        assert!(value < bound);
        value
    }
}

fn held(item: Item) -> Option<ItemStack> {
    Some(ItemStack::new(item, 1).unwrap())
}

fn break_drops(block: Block, tool: Option<ItemStack>, rolls: &[u32]) -> Vec<ItemStack> {
    break_drops_with(block, 0, tool, rolls)
}

fn break_drops_with(
    block: Block,
    metadata: u8,
    tool: Option<ItemStack>,
    rolls: &[u32],
) -> Vec<ItemStack> {
    player_break_drops_with_metadata(
        block,
        metadata,
        tool,
        &mut Rolls {
            values: rolls,
            index: 0,
        },
    )
}

fn one(item: Item, data: u16) -> ItemStack {
    ItemStack::with_data(item, 1, data).unwrap()
}

fn block_item(block: Block) -> ItemStack {
    ItemStack::from_block(block, 1).unwrap()
}

#[test]
fn break_drops_follow_beta_tool_and_item_rules() {
    let pick = held(Item::WoodenPickaxe);
    let stone_pick = held(Item::StonePickaxe);
    let shovel = held(Item::WoodenShovel);
    let sword = held(Item::WoodenSword);
    let shears = held(Item::Shears);

    assert!(break_drops(Block::Stone, None, &[]).is_empty());
    assert_eq!(
        break_drops(Block::Stone, pick, &[]),
        vec![block_item(Block::Cobblestone)]
    );
    assert_eq!(
        break_drops(Block::Dirt, None, &[]),
        vec![block_item(Block::Dirt)]
    );
    assert_eq!(
        break_drops(Block::Grass, None, &[]),
        vec![block_item(Block::Dirt)]
    );
    assert_eq!(
        break_drops(Block::CoalOre, pick, &[]),
        vec![one(Item::Coal, 0)]
    );
    assert_eq!(
        break_drops(Block::DiamondOre, held(Item::IronPickaxe), &[]),
        vec![one(Item::Diamond, 0)]
    );
    assert_eq!(
        break_drops(Block::IronOre, stone_pick, &[]),
        vec![block_item(Block::IronOre)]
    );
    assert_eq!(
        break_drops(Block::GoldOre, held(Item::IronPickaxe), &[]),
        vec![block_item(Block::GoldOre)]
    );
    assert!(break_drops(Block::IronOre, None, &[]).is_empty());
    assert_eq!(
        break_drops(Block::LapisOre, stone_pick, &[0]),
        vec![one(Item::Dye, 4); 4]
    );
    assert_eq!(
        break_drops(Block::LapisOre, stone_pick, &[4]),
        vec![one(Item::Dye, 4); 8]
    );
    assert_eq!(
        break_drops(Block::RedstoneOre, held(Item::IronPickaxe), &[0]),
        vec![one(Item::Redstone, 0); 4]
    );
    assert_eq!(
        break_drops(Block::LitRedstoneOre, held(Item::IronPickaxe), &[1]),
        vec![one(Item::Redstone, 0); 5]
    );
    assert_eq!(
        break_drops(Block::Glowstone, pick, &[0]),
        vec![one(Item::GlowstoneDust, 0); 2]
    );
    assert_eq!(
        break_drops(Block::Glowstone, pick, &[2]),
        vec![one(Item::GlowstoneDust, 0); 4]
    );
    assert!(break_drops(Block::Glowstone, None, &[]).is_empty());
    assert_eq!(
        break_drops(Block::Clay, None, &[]),
        vec![one(Item::ClayBall, 0); 4]
    );
    assert_eq!(
        break_drops(Block::Gravel, None, &[0]),
        vec![one(Item::Flint, 0)]
    );
    assert_eq!(
        break_drops(Block::Gravel, None, &[1]),
        vec![block_item(Block::Gravel)]
    );
    assert!(break_drops(Block::Glass, None, &[]).is_empty());
    assert!(break_drops(Block::Ice, None, &[]).is_empty());
    assert!(break_drops(Block::Bookshelf, None, &[]).is_empty());

    assert!(break_drops(Block::Snow, None, &[]).is_empty());
    assert_eq!(
        break_drops(Block::Snow, shovel, &[]),
        vec![one(Item::Snowball, 0); 4]
    );
    assert!(break_drops(Block::SnowLayer, None, &[]).is_empty());
    assert_eq!(
        break_drops(Block::SnowLayer, shovel, &[]),
        vec![one(Item::Snowball, 0)]
    );
    assert!(
        natural_drops(
            Block::SnowLayer,
            &mut Rolls {
                values: &[],
                index: 0
            }
        )
        .is_empty()
    );

    assert!(break_drops(Block::Cobweb, None, &[]).is_empty());
    assert_eq!(
        break_drops(Block::Cobweb, sword, &[]),
        vec![one(Item::String, 0)]
    );
    assert_eq!(
        break_drops(Block::Cobweb, shears, &[]),
        vec![one(Item::String, 0)]
    );

    assert!(break_drops(Block::Leaves, None, &[1]).is_empty());
    assert_eq!(
        break_drops(Block::Leaves, None, &[0]),
        vec![one(Item::from_block(Block::Sapling).unwrap(), 0)]
    );
    assert_eq!(
        break_drops_with(Block::Leaves, 1, None, &[0]),
        vec![one(Item::from_block(Block::Sapling).unwrap(), 1)]
    );
    assert_eq!(
        break_drops(Block::Leaves, shears, &[]),
        vec![block_item(Block::Leaves)]
    );
    assert_eq!(
        break_drops_with(Block::Leaves, 1, shears, &[]),
        vec![one(Item::from_block(Block::Leaves).unwrap(), 1)]
    );
    assert_eq!(
        break_drops_with(Block::Leaves, 2, shears, &[]),
        vec![one(Item::from_block(Block::Leaves).unwrap(), 2)]
    );

    assert_eq!(
        break_drops(Block::DoubleStoneSlab, pick, &[]),
        vec![block_item(Block::StoneSlab); 2]
    );
    assert_eq!(
        break_drops(Block::WoodenStairs, None, &[]),
        vec![block_item(Block::WoodenPlanks)]
    );
    assert_eq!(
        break_drops(Block::CobblestoneStairs, pick, &[]),
        vec![block_item(Block::Cobblestone)]
    );
    assert!(break_drops(Block::CobblestoneStairs, None, &[]).is_empty());
    assert_eq!(
        break_drops(Block::Tnt, None, &[]),
        vec![block_item(Block::Tnt)]
    );
    assert_eq!(
        break_drops(Block::TallGrass, None, &[0]),
        vec![one(Item::Seeds, 0)]
    );
    assert!(break_drops(Block::TallGrass, None, &[1]).is_empty());
    assert_eq!(
        break_drops(Block::Crops, None, &[0, 0, 0]),
        vec![one(Item::Seeds, 0); 3]
    );
    assert!(break_drops(Block::Crops, None, &[1, 1, 1]).is_empty());
}

#[test]
fn break_drops_preserve_wood_species_and_clear_torch_facing() {
    assert_eq!(
        break_drops_with(Block::Wood, 1, None, &[]),
        vec![one(Item::from_block(Block::Wood).unwrap(), 1)]
    );
    assert_eq!(
        break_drops_with(Block::Wood, 2, None, &[]),
        vec![one(Item::from_block(Block::Wood).unwrap(), 2)]
    );
    assert_eq!(
        natural_drops_with_metadata(
            Block::Torch,
            Block::Torch.facing_metadata(Direction::West),
            &mut Rolls {
                values: &[],
                index: 0
            }
        ),
        vec![block_item(Block::Torch)]
    );
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
        item_slipperiness(Some(Block::Ice)),
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
    assert!((spawned.x + 0.01).abs() < 1e-5);
    assert!((spawned.y - 0.2).abs() < 1e-5);
    assert!((spawned.z - 0.01).abs() < 1e-5);

    let thrown = thrown_item_motion(Vec3::NEG_Z, 0.0, 0.0, 0.0, 0.0);
    assert!(thrown.x.abs() < 1e-5);
    assert!((thrown.y - 0.1).abs() < 1e-5);
    assert!((thrown.z + 0.3).abs() < 1e-5);
}

#[test]
fn dropped_item_motion_lerps_across_the_partial_tick() {
    let previous = Vec3::new(1.0, 2.0, 3.0);
    let current = Vec3::new(1.4, 1.8, 3.2);
    assert_eq!(interpolated_item_position(previous, current, 0.0), previous);
    assert_eq!(interpolated_item_position(previous, current, 1.0), current);
    let midway = interpolated_item_position(previous, current, 0.5);
    assert!((midway - Vec3::new(1.2, 1.9, 3.1)).length() < 1e-5);
}

#[test]
fn cubes_spin_on_y_and_sprites_only_face_the_camera() {
    assert!((item_spin_yaw(20.0, 0.0, 0.5) - 1.5).abs() < 1e-5);
    assert!((item_visual_yaw(true, 1.25, 0.4) - 1.25).abs() < 1e-5);
    assert!((item_visual_yaw(false, 1.25, 0.4) - 0.4).abs() < 1e-5);
    let pose = item_piece_transform(0.2, 0.4, 0.5, Vec3::ZERO, Vec3::ZERO);
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
    let dirt = dropped_block_meshes(Block::Dirt, 0, true, [0.2, 0.8, 0.3], [1.0, 1.0, 1.0]);
    assert_eq!(position_count(&dirt.body), 24);
    assert!(dirt.overlay.is_none());
    assert!(!dirt.cutout);

    let grass = dropped_block_meshes(Block::Grass, 0, true, [0.2, 0.8, 0.3], [1.0, 1.0, 1.0]);
    assert_eq!(position_count(grass.overlay.as_ref().unwrap()), 16);
    let fast = dropped_block_meshes(Block::Grass, 0, false, [0.2, 0.8, 0.3], [1.0, 1.0, 1.0]);
    assert!(fast.overlay.is_none());

    let positions = positions_of(&grass.body);
    assert!(positions.iter().all(|position| position[0].abs() <= 0.51));
    let uvs = uvs_of(&grass.body);
    assert_ne!(uvs[0], uvs[8]);

    let colors = colors_of(&dirt.body);
    assert!((colors[0][0] - 1.0).abs() < 1e-5);
    assert!((colors[4][0] - 0.55).abs() < 1e-5);

    let leaves = dropped_block_meshes(Block::Leaves, 0, true, [1.0; 3], [0.2, 0.7, 0.1]);
    assert!(leaves.cutout);
    assert!(!dropped_block_meshes(Block::Leaves, 0, false, [1.0; 3], [0.2, 0.7, 0.1]).cutout);
}

#[test]
fn dropped_ladder_uses_the_flat_item_sprite() {
    let west = Block::Ladder.facing_metadata(Direction::West);
    let ladder = ItemStack::from_block_state(Block::Ladder, west, 1).unwrap();
    assert_eq!(ladder.runtime_block(), Some((Block::Ladder, 0)));
    assert_eq!(dropped_block_model(ladder), None);
}

#[test]
fn redstone_parts_drop_as_flat_sprites() {
    // `renderItemIn3d` only models a few render types; torches, repeaters, and
    // levers are item sprites, while solid blocks stay cubes.
    for block in [Block::RedstoneTorch, Block::Repeater, Block::Lever] {
        let stack = ItemStack::from_block_state(block, 0, 1).unwrap();
        assert_eq!(dropped_block_model(stack), None, "{block:?}");
    }
    let repeater = ItemStack::new(Item::Repeater, 1).unwrap();
    assert_eq!(dropped_block_model(repeater), None);
    let stone = ItemStack::from_block_state(Block::Stone, 0, 1).unwrap();
    assert_eq!(dropped_block_model(stone), Some((Block::Stone, 0)));
}

fn position_count(mesh: &BlockGeometry) -> usize {
    mesh.vertex_count()
}

fn positions_of(mesh: &BlockGeometry) -> Vec<[f32; 3]> {
    mesh.positions()
}

fn uvs_of(mesh: &BlockGeometry) -> Vec<[f32; 2]> {
    mesh.uvs()
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
    let full = ItemStack::from_block(Block::Dirt, 64).unwrap();
    hotbar.slots = [Some(full); 9];
    inventory.main = [Some(full); MAIN_SLOTS];
    let incoming = ItemStack::from_block(Block::Dirt, 3).unwrap();
    let remainder = inventory.insert(&mut hotbar, incoming).unwrap();
    assert_eq!(remainder.count(), incoming.count());
    assert!(hotbar.pop.iter().all(|pop| *pop == 0));
}

#[test]
fn a_partial_fit_pops_the_hotbar_slot_and_returns_the_rest() {
    let mut hotbar = Hotbar::default();
    let mut inventory = Inventory::default();
    let full = ItemStack::from_block(Block::Dirt, 64).unwrap();
    hotbar.slots = [Some(full); 9];
    hotbar.slots[3] = Some(ItemStack::from_block(Block::Dirt, 63).unwrap());
    inventory.main = [Some(full); MAIN_SLOTS];
    let remainder = inventory
        .insert(&mut hotbar, ItemStack::from_block(Block::Dirt, 5).unwrap())
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
    hotbar.slots[0] = Some(ItemStack::from_block(Block::Cobblestone, 2).unwrap());
    let taken = hotbar.take_selected(1).unwrap();
    assert_eq!(taken.count(), 1);
    assert_eq!(hotbar.selected_stack().unwrap().count(), 1);
    let last = hotbar.take_selected(1).unwrap();
    assert_eq!(last.count(), 1);
    assert!(hotbar.selected_stack().is_none());
}

/// Dropped blocks carry their face shade in the tint and ignore world light.
fn colors_of(mesh: &BlockGeometry) -> Vec<[f32; 4]> {
    mesh.colors(BlockLighting::default())
}

#[test]
fn dropped_items_drift_with_water_once_per_world_tick() {
    use bevy::asset::AssetPlugin;
    use bevy::mesh::MeshPlugin;
    use bevy::prelude::*;
    use bevy::state::app::StatesPlugin;
    use game::app::settings::GameSettings;
    use game::app::state::AppScreen;
    use game::entity::CollisionState;
    use game::entity::DroppedItem;
    use game::entity::PreviousTick;
    use game::entity::drops::items::DroppedItemPlugin;
    use game::entity::drops::items::DroppedItemState;
    use game::entity::drops::items::ItemMotion;
    use game::world::biome::Biome;
    use game::world::biome::BiomeMap;
    use game::world::biome::Climate;
    use game::world::chunk::Chunk;
    use game::world::chunk::ChunkPosition;
    use game::world::chunk::GeneratedChunk;
    use game::world::chunk::Heightmap;
    use game::world::chunk::WorldChunks;
    use game::world::tick::WorldTick;

    let mut chunk = Chunk::new();
    chunk.set(8, 65, 8, Block::Water);
    chunk.set_with_metadata(9, 65, 8, Block::FlowingWater, 1);
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPosition::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            chunk,
            biomes: BiomeMap::from_cells(
                [Climate {
                    temperature: 0.5,
                    humidity: 0.5,
                    biome: Biome::Plains,
                }; 16 * 16],
            ),
            items: Vec::new(),
            populated: true,
        },
    );
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        StatesPlugin,
    ))
    .init_asset::<StandardMaterial>()
    .init_state::<AppScreen>()
    .init_resource::<GameSettings>()
    .init_resource::<WorldTick>()
    .insert_resource(chunks)
    .add_plugins(DroppedItemPlugin);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();

    let spawn = |app: &mut App, y: f32| {
        app.world_mut()
            .spawn((
                DroppedItem(ItemStack::from_block(Block::Cobblestone, 1).unwrap()),
                DroppedItemState::new(100, 0.0, 3),
                Transform::from_xyz(8.5, y, 8.5),
                PreviousTick(Vec3::new(8.5, y, 8.5)),
                ItemMotion(Vec3::ZERO),
                CollisionState::default(),
                EntitySize::DROPPED_ITEM,
            ))
            .id()
    };
    let wet = spawn(&mut app, 65.5);
    let dry = spawn(&mut app, 66.5);
    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();
    let wet_pos = app
        .world()
        .entity(wet)
        .get::<Transform>()
        .unwrap()
        .translation;
    let dry_pos = app
        .world()
        .entity(dry)
        .get::<Transform>()
        .unwrap()
        .translation;
    assert!((wet_pos.x - 8.514).abs() < 1e-4);
    assert_eq!(dry_pos.x, 8.5);

    app.world_mut().resource_mut::<WorldTick>().advance(0.1);
    app.update();
    let wet_pos = app
        .world()
        .entity(wet)
        .get::<Transform>()
        .unwrap()
        .translation;
    assert!(wet_pos.x > 8.54, "two catch-up ticks apply two more pushes");
}

#[test]
fn zero_hotbar_pop_does_not_signal_a_change_and_active_pop_still_finishes() {
    use bevy::asset::AssetPlugin;
    use bevy::mesh::MeshPlugin;
    use bevy::prelude::*;
    use bevy::state::app::StatesPlugin;
    use game::app::settings::GameSettings;
    use game::app::state::AppScreen;
    use game::entity::drops::items::DroppedItemPlugin;
    use game::world::chunk::WorldChunks;
    use game::world::tick::WorldTick;

    #[derive(Resource, Default)]
    struct Changes(usize);
    fn observe(query: Query<(), Changed<Hotbar>>, mut changes: ResMut<Changes>) {
        changes.0 = query.iter().count();
    }
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        StatesPlugin,
    ))
    .init_asset::<StandardMaterial>()
    .init_state::<AppScreen>()
    .init_resource::<GameSettings>()
    .init_resource::<WorldTick>()
    .init_resource::<WorldChunks>()
    .init_resource::<Changes>()
    .add_plugins(DroppedItemPlugin)
    .add_systems(Last, observe);
    let entity = app.world_mut().spawn(Hotbar::default()).id();
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();
    app.update();
    assert_eq!(app.world().resource::<Changes>().0, 0);
    app.world_mut().get_mut::<Hotbar>(entity).unwrap().pop[0] = 5;
    app.update();
    assert_eq!(app.world().get::<Hotbar>(entity).unwrap().pop[0], 4);
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(app.world().get::<Hotbar>(entity).unwrap().pop[0], 0);
    app.update();
    assert_eq!(app.world().resource::<Changes>().0, 0);
    app.world_mut().get_mut::<Hotbar>(entity).unwrap().pop[1] = 5;
    app.world_mut().resource_mut::<WorldTick>().advance(0.25);
    app.update();
    assert_eq!(app.world().get::<Hotbar>(entity).unwrap().pop[1], 0);
}

#[test]
fn a_double_slab_drops_two_slabs_of_its_own_material() {
    let drops = natural_drops_with_metadata(
        Block::DoubleStoneSlab,
        2,
        &mut Rolls {
            values: &[],
            index: 0,
        },
    );
    let wooden = ItemStack::from_block_state(Block::StoneSlab, 2, 1).unwrap();
    assert_eq!(drops, vec![wooden; 2]);
    assert_eq!(wooden.data(), 2);
}

#[test]
fn fire_wears_an_item_down_a_point_a_tick() {
    use game::entity::drops::items::DroppedItemState;

    let mut state = DroppedItemState::new(0, 0.0, 1);
    for _ in 0..4 {
        state.update_hazards(false, false);
        state.contact_hazards(false, true, false);
        assert!(!state.is_destroyed());
    }
    assert!(state.fire > 0, "standing in fire sets the item alight");
    state.update_hazards(false, false);
    state.contact_hazards(false, true, false);
    assert!(state.is_destroyed());
}

#[test]
fn lava_destroys_an_item_at_once_and_cactus_in_five_ticks() {
    use game::entity::drops::items::DroppedItemState;

    // `setOnFireFromLava` takes four, and the lava it sits in the fifth.
    let mut state = DroppedItemState::new(0, 0.0, 1);
    state.update_hazards(false, true);
    assert_eq!((state.health, state.fire), (1, 600));
    state.contact_hazards(false, true, false);
    assert!(state.is_destroyed());

    let mut state = DroppedItemState::new(0, 0.0, 1);
    for _ in 0..4 {
        state.update_hazards(false, false);
        state.contact_hazards(true, false, false);
        assert!(!state.is_destroyed());
    }
    state.contact_hazards(true, false, false);
    assert!(state.is_destroyed());
}

#[test]
fn a_burning_item_keeps_burning_until_water_puts_it_out() {
    use game::entity::drops::items::DroppedItemState;

    let mut state = DroppedItemState::new(0, 0.0, 1);
    state.fire = 45;
    // Out of the fire it still loses a point each time the count passes a
    // multiple of twenty.
    for _ in 0..10 {
        state.update_hazards(false, false);
        state.contact_hazards(false, false, false);
    }
    assert_eq!((state.health, state.fire), (4, 35));
    state.update_hazards(true, false);
    state.contact_hazards(false, false, true);
    assert!(state.fire <= 0);
    assert_eq!(state.health, 4);
}

#[test]
fn an_item_dropped_into_fire_or_onto_a_cactus_is_destroyed() {
    use bevy::asset::AssetPlugin;
    use bevy::mesh::MeshPlugin;
    use bevy::prelude::*;
    use bevy::state::app::StatesPlugin;
    use game::app::settings::GameSettings;
    use game::app::state::AppScreen;
    use game::entity::CollisionState;
    use game::entity::DroppedItem;
    use game::entity::PreviousTick;
    use game::entity::drops::items::DroppedItemPlugin;
    use game::entity::drops::items::DroppedItemState;
    use game::entity::drops::items::ItemMotion;
    use game::world::biome::Biome;
    use game::world::biome::BiomeMap;
    use game::world::biome::Climate;
    use game::world::chunk::Chunk;
    use game::world::chunk::ChunkPosition;
    use game::world::chunk::GeneratedChunk;
    use game::world::chunk::Heightmap;
    use game::world::chunk::WorldChunks;
    use game::world::tick::WorldTick;

    let mut chunk = Chunk::new();
    for x in 2..14 {
        for z in 2..14 {
            chunk.set(x, 64, z, Block::Stone);
        }
    }
    chunk.set(4, 65, 4, Block::Fire);
    chunk.set(8, 64, 8, Block::Sand);
    chunk.set(8, 65, 8, Block::Cactus);
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPosition::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            chunk,
            biomes: BiomeMap::from_cells(
                [Climate {
                    temperature: 0.5,
                    humidity: 0.5,
                    biome: Biome::Plains,
                }; 16 * 16],
            ),
            items: Vec::new(),
            populated: true,
        },
    );
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        StatesPlugin,
    ))
    .init_asset::<StandardMaterial>()
    .init_state::<AppScreen>()
    .init_resource::<GameSettings>()
    .init_resource::<WorldTick>()
    .insert_resource(chunks)
    .add_plugins(DroppedItemPlugin);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();

    let spawn = |app: &mut App, position: Vec3| {
        app.world_mut()
            .spawn((
                DroppedItem(ItemStack::from_block(Block::Cobblestone, 1).unwrap()),
                DroppedItemState::new(100, 0.0, 3),
                Transform::from_translation(position),
                PreviousTick(position),
                ItemMotion(Vec3::ZERO),
                CollisionState::default(),
                EntitySize::DROPPED_ITEM,
            ))
            .id()
    };
    let burning = spawn(&mut app, Vec3::new(4.5, 65.2, 4.5));
    let pricked = spawn(&mut app, Vec3::new(8.5, 66.2, 8.5));
    let safe = spawn(&mut app, Vec3::new(11.5, 65.2, 11.5));
    for _ in 0..30 {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.update();
    }
    assert!(app.world().get_entity(burning).is_err());
    assert!(app.world().get_entity(pricked).is_err());
    assert!(app.world().get_entity(safe).is_ok());
}
