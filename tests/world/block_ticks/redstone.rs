use super::TestWorld;
use super::at;
use bevy::math::IVec3;
use game::block::blocks::Block;
use game::world::block_ticks::BlockEvent;

fn place_meta(world: &mut TestWorld, pos: IVec3, block: Block, meta: u8) {
    let old = world.block(pos);
    let old_meta = world.metadata(pos);
    world.set_with_metadata(pos, block, meta);
    world.ticks.block_changed(pos, old, old_meta);
    world.process_events();
}

#[test]
fn switch_wire_repeater_and_button_timing() {
    let mut w = TestWorld::new(1);
    w.fill(at(2, 63, 2), at(9, 63, 9), Block::Stone);
    let lever = at(4, 64, 4);
    let wire = at(5, 64, 4);
    let second = at(6, 64, 4);
    let repeater = at(7, 64, 4);
    place_meta(&mut w, lever, Block::Lever, 5);
    w.place(wire, Block::RedstoneWire);
    w.place(second, Block::RedstoneWire);
    place_meta(&mut w, repeater, Block::Repeater, 1);
    w.event(BlockEvent::Activated { position: lever });
    assert_eq!(w.metadata(wire), 15);
    assert_eq!(w.metadata(second), 14);
    assert_eq!(w.block(repeater), Block::Repeater);
    w.run(1);
    assert_eq!(w.block(repeater), Block::Repeater);
    w.run(1);
    assert_eq!(w.block(repeater), Block::PoweredRepeater);
    w.event(BlockEvent::Activated { position: repeater });
    assert_eq!(w.metadata(repeater) >> 2, 1);
    w.event(BlockEvent::Activated { position: lever });
    assert_eq!(w.metadata(wire), 0);
    assert_eq!(w.metadata(second), 0);
    w.run(3);
    assert_eq!(w.block(repeater), Block::PoweredRepeater);
    w.run(1);
    assert_eq!(w.block(repeater), Block::Repeater);

    let button = at(4, 64, 6);
    w.set(at(3, 64, 6), Block::Stone);
    place_meta(&mut w, button, Block::StoneButton, 1);
    w.event(BlockEvent::Activated { position: button });
    assert_eq!(w.metadata(button), 9);
    w.run(19);
    assert_eq!(w.metadata(button), 9);
    w.run(1);
    assert_eq!(w.metadata(button), 1);
}

#[test]
fn doors_toggle_both_halves_and_lose_support() {
    let mut w = TestWorld::new(1);
    let bottom = at(8, 70, 8);
    let upper = bottom + IVec3::Y;
    w.set(bottom - IVec3::Y, Block::Stone);
    place_meta(&mut w, bottom, Block::WoodenDoor, 1);
    place_meta(&mut w, upper, Block::WoodenDoor, 9);
    w.event(BlockEvent::Activated { position: upper });
    assert_ne!(w.metadata(bottom) & 4, 0);
    assert_ne!(w.metadata(upper) & 4, 0);
    w.place(bottom - IVec3::Y, Block::Air);
    assert_eq!(w.block(bottom), Block::Air);
    assert_eq!(w.block(upper), Block::Air);
}

#[test]
fn pressure_plates_filter_entities_and_reset_after_delay() {
    use game::world::block_ticks::RedstoneOccupant;
    let mut w = TestWorld::new(1);
    let stone = at(4, 65, 4);
    let wood = at(6, 65, 4);
    w.fill(at(3, 64, 3), at(7, 64, 5), Block::Stone);
    w.place(stone, Block::StonePressurePlate);
    w.place(wood, Block::WoodenPressurePlate);
    let occupant = |x: f32, living| RedstoneOccupant {
        min: [x, 65.0, 4.3],
        max: [x + 0.25, 65.25, 4.55],
        living,
        minecart: false,
    };
    w.ticks
        .set_occupants(vec![occupant(4.3, false), occupant(6.3, false)]);
    w.run(1);
    assert_eq!(w.metadata(stone), 0);
    assert_eq!(w.metadata(wood), 1);
    w.ticks.set_occupants(vec![occupant(4.3, true)]);
    w.run(1);
    assert_eq!(w.metadata(stone), 1);
    w.ticks.set_occupants(vec![]);
    w.run(19);
    assert_eq!(w.metadata(stone), 1);
    w.run(1);
    assert_eq!(w.metadata(stone), 0);
    assert_eq!(w.metadata(wood), 0);
}

#[test]
fn piston_pushes_and_sticky_piston_pulls_without_duplication() {
    let mut w = TestWorld::new(1);
    let piston = at(8, 64, 8);
    let lever = piston + IVec3::NEG_Z;
    let first = piston + IVec3::X;
    place_meta(&mut w, piston, Block::StickyPiston, 5);
    w.set(first, Block::Cobblestone);
    w.set(first + IVec3::X, Block::Stone);
    w.set(lever - IVec3::Y, Block::Stone);
    place_meta(&mut w, lever, Block::Lever, 5);
    w.event(BlockEvent::Activated { position: lever });
    assert_eq!(w.block(first), Block::PistonHead);
    assert_eq!(w.block(first + IVec3::X), Block::Cobblestone);
    assert_eq!(w.block(first + IVec3::X * 2), Block::Stone);
    w.event(BlockEvent::Activated { position: lever });
    assert_eq!(w.block(first), Block::Cobblestone);
    assert_eq!(w.block(first + IVec3::X), Block::Air);
    assert_eq!(w.block(first + IVec3::X * 2), Block::Stone);
}

#[test]
fn torch_below_stone_powers_and_releases_a_downward_piston_beside_it() {
    let mut w = TestWorld::new(1);
    let piston = at(8, 65, 8);
    let stone = piston + IVec3::NEG_X;
    let torch = stone + IVec3::NEG_Y;
    // The torch strongly powers the stone above it. That stone is next to
    // the piston, but the torch itself is diagonal from the piston.
    w.set(torch + IVec3::NEG_Y, Block::Stone);
    w.set(stone, Block::Stone);
    w.set(piston + IVec3::NEG_Y, Block::Stone);
    place_meta(&mut w, piston, Block::Piston, 0);
    assert_eq!(w.metadata(piston), 0);

    place_meta(&mut w, torch, Block::RedstoneTorch, 5);
    assert_eq!(w.block(torch), Block::RedstoneTorch);
    assert_eq!(w.metadata(piston), 8, "piston should extend downward");
    assert_eq!(w.block(piston + IVec3::NEG_Y), Block::PistonHead);
    assert_eq!(w.block(piston + IVec3::NEG_Y * 2), Block::Stone);

    // Switching the torch off and back on must also notify the piston,
    // even though neither torch state touches the piston directly.
    let lever = torch + IVec3::NEG_Y + IVec3::NEG_X;
    place_meta(&mut w, lever, Block::Lever, 2);
    w.event(BlockEvent::Activated { position: lever });
    w.run(2);
    assert_eq!(w.block(torch), Block::UnlitRedstoneTorch);
    assert_eq!(w.metadata(piston), 0);
    w.event(BlockEvent::Activated { position: lever });
    w.run(2);
    assert_eq!(w.block(torch), Block::RedstoneTorch);
    assert_eq!(w.metadata(piston), 8);

    w.place(torch, Block::Air);
    assert_eq!(
        w.metadata(piston),
        0,
        "piston should retract when the torch is removed"
    );
    assert_eq!(w.block(piston + IVec3::NEG_Y), Block::Air);
}

#[test]
fn detector_rail_only_responds_to_carts() {
    use game::world::block_ticks::RedstoneOccupant;
    let mut w = TestWorld::new(1);
    let rail = at(8, 65, 8);
    w.set(rail - IVec3::Y, Block::Stone);
    w.place(rail, Block::DetectorRail);
    let mut entity = RedstoneOccupant {
        min: [8.3, 65.0, 8.3],
        max: [8.6, 65.2, 8.6],
        living: true,
        minecart: false,
    };
    w.ticks.set_occupants(vec![entity]);
    w.run(1);
    assert_eq!(w.metadata(rail) & 8, 0);
    entity.minecart = true;
    w.ticks.set_occupants(vec![entity]);
    w.run(1);
    assert_eq!(w.metadata(rail) & 8, 8);
    w.ticks.set_occupants(vec![]);
    w.run(20);
    assert_eq!(w.metadata(rail) & 8, 0);
}

#[test]
fn torch_inverts_power_with_two_tick_delay() {
    let mut w = TestWorld::new(1);
    let support = at(8, 64, 8);
    let torch = support + IVec3::Y;
    let lever = support + IVec3::X;
    w.set(support, Block::Stone);
    place_meta(&mut w, lever, Block::Lever, 1);
    place_meta(&mut w, torch, Block::RedstoneTorch, 5);
    w.event(BlockEvent::Activated { position: lever });
    w.run(1);
    assert_eq!(w.block(torch), Block::RedstoneTorch);
    w.run(1);
    assert_eq!(w.block(torch), Block::UnlitRedstoneTorch);
    w.event(BlockEvent::Activated { position: lever });
    w.run(2);
    assert_eq!(w.block(torch), Block::RedstoneTorch);
}

#[test]
fn note_block_advances_pitch_and_plays_only_on_rising_edge() {
    use game::world::block_ticks::TickEffect;
    let mut w = TestWorld::new(1);
    let note = at(8, 64, 8);
    let lever = note + IVec3::X;
    w.set(note - IVec3::Y, Block::WoodenPlanks);
    w.place(note, Block::NoteBlock);
    place_meta(&mut w, lever, Block::Lever, 1);
    w.event(BlockEvent::Activated { position: note });
    assert_eq!(
        w.chunks.note_at_mut(note.x, note.y, note.z).unwrap().pitch,
        1
    );
    assert!(w.effects().contains(&TickEffect::Note {
        position: note,
        instrument: 4,
        pitch: 1
    }));
    w.event(BlockEvent::Activated { position: lever });
    assert_eq!(w.effects().len(), 1);
    w.event(BlockEvent::Activated { position: lever });
    assert!(w.effects().is_empty());
}

#[test]
fn long_wire_runs_stop_after_fifteen_cells_and_cross_chunk_boundaries() {
    let mut w = TestWorld::new(2);
    w.fill(at(0, 63, 4), at(31, 63, 4), Block::Stone);
    place_meta(&mut w, at(1, 64, 4), Block::Lever, 5);
    for x in 2..=26 {
        w.place(at(x, 64, 4), Block::RedstoneWire);
    }
    w.event(BlockEvent::Activated {
        position: at(1, 64, 4),
    });
    assert_eq!(w.metadata(at(2, 64, 4)), 15);
    assert_eq!(w.metadata(at(16, 64, 4)), 1);
    assert_eq!(w.metadata(at(17, 64, 4)), 0);
    w.event(BlockEvent::Activated {
        position: at(1, 64, 4),
    });
    assert_eq!(w.metadata(at(2, 64, 4)), 0);
    assert_eq!(w.metadata(at(16, 64, 4)), 0);
}

#[test]
fn piston_refuses_a_thirteenth_push_and_unloaded_destinations() {
    let mut w = TestWorld::new(1);
    let base = at(8, 64, 8);
    let lever = base + IVec3::NEG_Z;
    place_meta(&mut w, base, Block::Piston, 5);
    for x in 9..=21 {
        w.set(at(x, 64, 8), Block::Stone);
    }
    w.set(lever - IVec3::Y, Block::Stone);
    place_meta(&mut w, lever, Block::Lever, 5);
    w.event(BlockEvent::Activated { position: lever });
    assert_eq!(w.metadata(base) & 8, 0);
    assert_eq!(w.block(base + IVec3::X), Block::Stone);
}

#[test]
fn rapid_redstone_torch_cycles_burn_out_until_history_expires() {
    let mut w = TestWorld::new(1);
    let base = at(8, 64, 8);
    let lever = base + IVec3::X;
    let torch = base + IVec3::Y;
    w.set(base, Block::Stone);
    place_meta(&mut w, lever, Block::Lever, 1);
    place_meta(&mut w, torch, Block::RedstoneTorch, 5);
    for cycle in 0..8 {
        w.event(BlockEvent::Activated { position: lever });
        w.run(2);
        assert_eq!(w.block(torch), Block::UnlitRedstoneTorch);
        w.event(BlockEvent::Activated { position: lever });
        w.run(2);
        assert_eq!(
            w.block(torch),
            if cycle == 7 {
                Block::UnlitRedstoneTorch
            } else {
                Block::RedstoneTorch
            }
        );
    }
    w.run(100);
    w.random_ticks(torch, 1);
    assert_eq!(w.block(torch), Block::RedstoneTorch);
}

#[test]
fn powered_tnt_primes_with_an_eighty_tick_fuse() {
    use game::world::block_ticks::TickEffect;

    let mut w = TestWorld::new(1);
    let charge = at(8, 64, 8);
    let lever = charge + IVec3::NEG_X;
    w.set(lever - IVec3::Y, Block::Stone);
    place_meta(&mut w, lever, Block::Lever, 5);
    w.place(charge, Block::Tnt);
    w.event(BlockEvent::Activated { position: lever });
    assert_eq!(w.block(charge), Block::Air);
    assert!(w.effects().contains(&TickEffect::PrimedTnt {
        position: charge,
        fuse: 80,
    }));
}

#[test]
fn powered_dispenser_takes_one_item_after_four_ticks() {
    use game::item::Item;
    use game::item::ItemStack;
    use game::world::block_ticks::TickEffect;

    let mut w = TestWorld::new(1);
    let dispenser = at(8, 64, 8);
    let lever = dispenser + IVec3::NEG_X;
    w.set(lever - IVec3::Y, Block::Stone);
    place_meta(&mut w, lever, Block::Lever, 5);
    w.place(dispenser, Block::Dispenser);
    let stack = ItemStack::new(Item::Egg, 2).unwrap();
    w.chunks.dispenser_at_mut(8, 64, 8).unwrap().slots[3] = Some(stack);
    w.event(BlockEvent::Activated { position: lever });
    w.run(3);
    assert!(w.effects().is_empty());
    w.run(1);
    assert_eq!(
        w.chunks.dispenser_at(8, 64, 8).unwrap().slots[3]
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        w.effects(),
        vec![TickEffect::Dispense {
            position: dispenser,
            facing: w.metadata(dispenser),
            stack: ItemStack::new(Item::Egg, 1).unwrap(),
        }]
    );
}

#[test]
fn rails_connect_corners_slopes_and_switch_junctions() {
    let mut w = TestWorld::new(1);
    w.fill(at(5, 63, 5), at(11, 64, 11), Block::Stone);
    let corner = at(8, 65, 8);
    w.place(corner, Block::Rail);
    w.place(corner + IVec3::Z, Block::Rail);
    w.place(corner + IVec3::X, Block::Rail);
    assert_eq!(w.metadata(corner), 6, "south-east bend");

    let slope = at(8, 65, 10);
    w.place(slope, Block::PoweredRail);
    w.set(slope + IVec3::Z + IVec3::Y - IVec3::Y, Block::Stone);
    w.place(slope + IVec3::Z + IVec3::Y, Block::PoweredRail);
    assert_eq!(w.metadata(slope) & 7, 5, "rises to the south");
}

#[test]
fn powered_rail_launches_cart_away_from_a_solid_block() {
    use bevy::math::Vec3;
    use game::entity::minecart::Minecart;
    use game::entity::minecart::step_minecart;

    let mut w = TestWorld::new(1);
    w.set(at(7, 64, 8), Block::Stone);
    for x in 8..=12 {
        w.set_with_metadata(at(x, 64, 8), Block::PoweredRail, 9);
    }
    let mut cart = Minecart::default();
    let mut center = Vec3::new(8.5, 64.35, 8.5);
    for _ in 0..6 {
        step_minecart(&mut cart, &mut center, &w.chunks);
    }
    assert!(cart.motion.x > 0.01);
    assert!(center.x > 8.7);
}

#[test]
fn powered_rail_signal_travels_at_most_eight_tracks() {
    let mut w = TestWorld::new(1);
    w.fill(at(3, 63, 8), at(16, 63, 8), Block::Stone);
    let lever = at(4, 64, 8);
    place_meta(&mut w, lever, Block::Lever, 5);
    for x in 5..=15 {
        w.place(at(x, 64, 8), Block::PoweredRail);
    }
    w.event(BlockEvent::Activated { position: lever });
    assert_ne!(w.metadata(at(12, 64, 8)) & 8, 0);
    assert_ne!(w.metadata(at(13, 64, 8)) & 8, 0);
    assert_eq!(w.metadata(at(14, 64, 8)) & 8, 0);
}
