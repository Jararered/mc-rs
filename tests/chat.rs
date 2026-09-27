use bevy::prelude::*;
use game::block::id::Id;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::ItemId;
use game::item::ItemStack;
use game::ui::screens::chat::ChatCommand;
use game::ui::screens::chat::give_to_inventory;
use game::ui::screens::chat::parse_command;
use game::ui::screens::chat::set_loaded_block;
use game::world::block_ticks::BlockTicks;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;

#[test]
fn parses_numeric_beta_ids_and_three_commands() {
    assert_eq!(
        parse_command("/give 264 70").unwrap(),
        ChatCommand::Give {
            item: ItemId::Diamond,
            amount: 70
        }
    );
    assert_eq!(
        parse_command("/tp -12.5 75 3").unwrap(),
        ChatCommand::Teleport(Vec3::new(-12.5, 75.0, 3.0))
    );
    assert_eq!(
        parse_command("/setblock -1 70 2 0").unwrap(),
        ChatCommand::SetBlock {
            position: IVec3::new(-1, 70, 2),
            block: Id::Air
        }
    );
    assert_eq!(
        parse_command("/setblock 1 4 3 1").unwrap(),
        ChatCommand::SetBlock {
            position: IVec3::new(1, 4, 3),
            block: Id::Stone
        }
    );
}

#[test]
fn bad_arguments_are_rejected_without_running_a_command() {
    for line in [
        "/give 999 1",
        "/give 0 1",
        "/give 264 0",
        "/give 264 4097",
        "/give 264",
        "/tp 1 2",
        "/tp 1 NaN 3",
        "/tp 1 inf 3",
        "/setblock 1 128 3 1",
        "/setblock 1.2 10 3 1",
        "/setblock 1 10 3 255",
        "/setblock 1 10 3 92",
        "/unknown 1",
    ] {
        assert!(parse_command(line).is_err(), "accepted {line}");
    }
}

#[test]
fn giving_items_stacks_and_returns_only_overflow() {
    let mut hotbar = Hotbar::default();
    let mut inventory = Inventory::default();
    hotbar.slots[0] = Some(ItemStack::new(ItemId::Diamond, 60).unwrap());
    assert!(give_to_inventory(ItemId::Diamond, 70, &mut hotbar, &mut inventory).is_empty());
    assert_eq!(hotbar.slots[0].unwrap().count(), 64);
    assert_eq!(hotbar.slots[1].unwrap().count(), 64);
    assert_eq!(hotbar.slots[2].unwrap().count(), 2);
    // Full inventory returns all requested items, including unstackable tools.
    hotbar
        .slots
        .fill(Some(ItemStack::new(ItemId::IronPickaxe, 1).unwrap()));
    inventory
        .main
        .fill(Some(ItemStack::new(ItemId::IronPickaxe, 1).unwrap()));
    let overflow = give_to_inventory(ItemId::IronPickaxe, 3, &mut hotbar, &mut inventory);
    assert_eq!(overflow.len(), 3);
    assert!(overflow.iter().all(|stack| stack.count() == 1));
}

fn world_with(chunk: Chunk) -> WorldChunks {
    let mut world = WorldChunks::default();
    world.insert(
        ChunkPosition::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            biomes: BiomeMap::from_cells(
                [Climate {
                    temperature: 0.5,
                    humidity: 0.5,
                    biome: Biome::Plains,
                }; CHUNK_SIZE * CHUNK_SIZE],
            ),
            chunk,
            items: Vec::new(),
            populated: true,
        },
    );
    world
}

#[test]
fn setblock_changes_only_loaded_cells_and_preserves_previous_metadata_for_ticks() {
    let mut chunk = Chunk::new();
    chunk.set_with_metadata(1, 70, 2, Id::Crops, 7);
    let mut world = world_with(chunk);
    let pos = IVec3::new(1, 70, 2);
    assert_eq!(
        set_loaded_block(&mut world, pos, Id::Stone),
        Ok(Some((Id::Crops, 7)))
    );
    assert_eq!(world.block_at(1, 70, 2), Some(Id::Stone));
    assert_eq!(world.metadata_at(1, 70, 2), 0);
    assert_eq!(set_loaded_block(&mut world, pos, Id::Stone), Ok(None));
    assert!(set_loaded_block(&mut world, IVec3::new(20, 70, 2), Id::Stone).is_err());
    assert!(set_loaded_block(&mut world, IVec3::new(1, 128, 2), Id::Stone).is_err());
    let mut ticks = BlockTicks::default();
    ticks.block_changed(pos, Id::Crops, 7);
    assert!(ticks.has_pending_events());
}

#[test]
fn keyboard_focus_open_submit_and_escape_behave_like_chat() {
    use bevy::input::ButtonState;
    use bevy::input::keyboard::KeyboardInput;
    use bevy::state::app::StatesPlugin;
    use bevy::window::CursorGrabMode;
    use bevy::window::CursorOptions;
    use bevy::window::PrimaryWindow;
    use game::app::state::AppScreen;
    use game::ui::ChatPlugin;
    use game::ui::icons::overlay::UiFont;
    use game::ui::screens::chat::ChatState;
    use game::world::tick::WorldTick;

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin))
        .init_state::<AppScreen>()
        .add_message::<KeyboardInput>()
        .insert_resource(ButtonInput::<KeyCode>::default())
        .insert_resource(WorldTick::default())
        .insert_resource(WorldChunks::default())
        .insert_resource(UiFont {
            minecraft: Handle::default(),
        })
        .add_plugins(ChatPlugin);
    let window = app
        .world_mut()
        .spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
            CursorOptions {
                grab_mode: CursorGrabMode::Locked,
                ..default()
            },
        ))
        .id();
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyT);
    app.update();
    assert!(app.world().resource::<ChatState>().open);
    assert!(app.world().resource::<ChatState>().suppress_controls);
    assert_eq!(
        app.world()
            .entity(window)
            .get::<CursorOptions>()
            .unwrap()
            .grab_mode,
        CursorGrabMode::None
    );
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::KeyH,
        logical_key: bevy::input::keyboard::Key::Character("h".into()),
        state: ButtonState::Pressed,
        text: Some("hello".into()),
        repeat: false,
        window,
    });
    app.update();
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Enter,
        logical_key: bevy::input::keyboard::Key::Enter,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    app.update();
    assert!(!app.world().resource::<ChatState>().open);
    assert!(app.world().resource::<ChatState>().suppress_controls);
    assert_eq!(
        app.world()
            .entity(window)
            .get::<CursorOptions>()
            .unwrap()
            .grab_mode,
        CursorGrabMode::Locked
    );
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "<Player> hello")
    );
    app.update();
    assert!(!app.world().resource::<ChatState>().suppress_controls);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Slash);
    app.update();
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.starts_with("> /"))
    );
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Escape,
        logical_key: bevy::input::keyboard::Key::Escape,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    app.update();
    assert!(!app.world().resource::<ChatState>().open);
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("Unknown command"))
    );
}

#[test]
fn submitted_commands_change_player_inventory_and_world() {
    use bevy::input::ButtonState;
    use bevy::input::keyboard::KeyboardInput;
    use bevy::state::app::StatesPlugin;
    use bevy::window::CursorGrabMode;
    use bevy::window::CursorOptions;
    use bevy::window::PrimaryWindow;
    use game::app::state::AppScreen;
    use game::player::Player;
    use game::ui::ChatPlugin;
    use game::ui::icons::overlay::UiFont;

    let mut chunk = Chunk::new();
    chunk.set(1, 70, 2, Id::Dirt);
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin))
        .init_state::<AppScreen>()
        .add_message::<KeyboardInput>()
        .insert_resource(ButtonInput::<KeyCode>::default())
        .insert_resource(game::world::tick::WorldTick::default())
        .insert_resource(UiFont {
            minecraft: Handle::default(),
        })
        .insert_resource(world_with(chunk))
        .insert_resource(BlockTicks::default())
        .add_plugins(ChatPlugin);
    let window = app
        .world_mut()
        .spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
            CursorOptions {
                grab_mode: CursorGrabMode::Locked,
                ..default()
            },
        ))
        .id();
    app.world_mut().spawn((
        Player,
        Hotbar::default(),
        Inventory::default(),
        Transform::from_xyz(8.0, 70.0, 8.0),
    ));
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();

    for command in ["/tp 5 80 6", "/give 264 70", "/setblock 1 70 2 1"] {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyT);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyT);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::Space,
            logical_key: bevy::input::keyboard::Key::Space,
            state: ButtonState::Pressed,
            text: Some(command.into()),
            repeat: false,
            window,
        });
        app.update();
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::Enter,
            logical_key: bevy::input::keyboard::Key::Enter,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        app.update();
    }
    let player = app
        .world_mut()
        .query_filtered::<(&Transform, &Hotbar), With<Player>>()
        .single(app.world())
        .unwrap();
    assert_eq!(player.0.translation, Vec3::new(5.0, 80.0, 6.0));
    assert_eq!(player.1.slots[0].unwrap().count(), 64);
    assert_eq!(player.1.slots[1].unwrap().count(), 6);
    assert_eq!(
        app.world().resource::<WorldChunks>().block_at(1, 70, 2),
        Some(Id::Stone)
    );
    assert!(app.world().resource::<BlockTicks>().has_pending_events());
}
