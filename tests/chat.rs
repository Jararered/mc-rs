use bevy::prelude::*;
use game::block::id::Id;
use game::chat::commands::ChatCommand;
use game::chat::commands::give_to_inventory;
use game::chat::commands::set_loaded_block;
use game::chat::registry::CommandRegistry;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::ItemId;
use game::item::ItemStack;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::block_ticks::BlockTicks;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;

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
    assert_eq!(
        parse_command("/wireframe on").unwrap(),
        ChatCommand::Wireframe {
            enabled: true,
            block: None
        }
    );
    assert_eq!(
        parse_command("/wireframe off").unwrap(),
        ChatCommand::Wireframe {
            enabled: false,
            block: None
        }
    );
    assert_eq!(
        parse_command("/wireframe set 9").unwrap(),
        ChatCommand::Wireframe {
            enabled: true,
            block: Some(Id::Water)
        }
    );
    assert_eq!(
        parse_command("/wireframe set 2").unwrap(),
        ChatCommand::Wireframe {
            enabled: true,
            block: Some(Id::Grass)
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
        "/wireframe",
        "/wireframe set",
        "/wireframe set 0",
        "/wireframe set 255",
        "/wireframe on extra",
        "/wireframe set 9 extra",
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
    use game::chat::ChatFocus;
    use game::chat::ChatPlugin;
    use game::ui::ChatUiPlugin;
    use game::ui::icons::overlay::UiFont;
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
        .add_plugins((ChatPlugin, ChatUiPlugin));
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
    assert!(app.world().resource::<ChatFocus>().open);
    assert!(app.world().resource::<ChatFocus>().suppress_controls);
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
    assert!(!app.world().resource::<ChatFocus>().open);
    assert!(app.world().resource::<ChatFocus>().suppress_controls);
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
    assert!(!app.world().resource::<ChatFocus>().suppress_controls);

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
    assert!(!app.world().resource::<ChatFocus>().open);
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
    use game::chat::ChatPlugin;
    use game::player::Player;
    use game::ui::ChatUiPlugin;
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
        .add_plugins((ChatPlugin, ChatUiPlugin));
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

#[test]
fn parses_help_and_time_commands() {
    use game::chat::commands::TimeQuery;

    assert_eq!(parse_command("/help"), Ok(ChatCommand::Help(None)));
    assert_eq!(
        parse_command("/help /time"),
        Ok(ChatCommand::Help(Some("time".into())))
    );
    for (name, time) in [
        ("day", 1000),
        ("noon", 6000),
        ("night", 13000),
        ("midnight", 18000),
    ] {
        assert_eq!(
            parse_command(&format!("/time {name}")),
            Ok(ChatCommand::TimeSet(time))
        );
        assert_eq!(
            parse_command(&format!("/time set {name}")),
            Ok(ChatCommand::TimeSet(time))
        );
    }
    for time in [0, 24000, u64::MAX] {
        assert_eq!(
            parse_command(&format!("/time set {time}")),
            Ok(ChatCommand::TimeSet(time))
        );
        assert_eq!(
            parse_command(&format!("/time {time}")),
            Ok(ChatCommand::TimeSet(time))
        );
    }
    assert_eq!(
        parse_command(" /time   add  50 "),
        Ok(ChatCommand::TimeAdd(50))
    );
    assert_eq!(
        parse_command("/time query"),
        Ok(ChatCommand::TimeQuery(TimeQuery::Daytime))
    );
    for (name, query) in [
        ("daytime", TimeQuery::Daytime),
        ("gametime", TimeQuery::Gametime),
        ("day", TimeQuery::Day),
    ] {
        assert_eq!(
            parse_command(&format!("/time query {name}")),
            Ok(ChatCommand::TimeQuery(query))
        );
    }
    for line in [
        "/help unknown",
        "/help time extra",
        "/time",
        "/time set",
        "/time add",
        "/time day extra",
        "/time set -1",
        "/time set 1.5",
        "/time set NaN",
        "/time set 18446744073709551616",
        "/time add -1",
        "/time add day",
        "/time query unknown",
        "/time query day extra",
    ] {
        assert!(parse_command(line).is_err(), "accepted {line}");
    }
}

fn submit_command(app: &mut App, window: Entity, command: &str) {
    use bevy::input::ButtonState;
    use bevy::input::keyboard::Key;
    use bevy::input::keyboard::KeyboardInput;

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
        logical_key: Key::Space,
        state: ButtonState::Pressed,
        text: Some(command.into()),
        repeat: false,
        window,
    });
    app.update();
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Enter,
        logical_key: Key::Enter,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    app.update();
}

#[test]
fn help_and_time_work_without_player_and_preserve_scheduled_delays() {
    use bevy::input::keyboard::KeyboardInput;
    use bevy::state::app::StatesPlugin;
    use bevy::window::CursorGrabMode;
    use bevy::window::CursorOptions;
    use bevy::window::PrimaryWindow;
    use game::app::state::AppScreen;
    use game::chat::ChatPlugin;
    use game::ui::ChatUiPlugin;
    use game::ui::icons::overlay::UiFont;
    use game::world::tick::WorldTick;

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin))
        .init_state::<AppScreen>()
        .add_message::<KeyboardInput>()
        .insert_resource(ButtonInput::<KeyCode>::default())
        .insert_resource(WorldTick::default())
        .insert_resource(WorldChunks::default())
        .insert_resource(BlockTicks::default())
        .insert_resource(UiFont {
            minecraft: Handle::default(),
        })
        .add_plugins((ChatPlugin, ChatUiPlugin));
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
        .resource_mut::<CommandRegistry>()
        .register(
            "clock",
            "Read the current daytime.",
            ["/clock"],
            parse_clock,
        )
        .unwrap();
    submit_command(&mut app, window, "/help");
    for command in ["/time", "/give", "/tp", "/setblock", "/wireframe", "/clock"] {
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0.starts_with(command)),
            "help missing {command}"
        );
    }
    submit_command(&mut app, window, "/help /clock");
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "Read the current daytime.")
    );
    submit_command(&mut app, window, "/clock");
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "Daytime: 0")
    );
    let pos = IVec3::new(1, 70, 2);
    app.world_mut()
        .resource_mut::<BlockTicks>()
        .schedule(pos, Id::FlowingWater, 7);
    app.world_mut().resource_mut::<WorldTick>().advance(0.1);
    for (command, expected) in [
        ("/time night", 13000),
        ("/time set 0", 0),
        ("/time set day", 1000),
        ("/time add 50", 1050),
    ] {
        submit_command(&mut app, window, command);
        assert_eq!(app.world().resource::<WorldTick>().world_time(), expected);
        let ticks = app.world().resource::<BlockTicks>();
        assert_eq!(ticks.time(), expected);
        assert_eq!(app.world().resource::<WorldTick>().ticks_this_frame(), 0);
        assert_eq!(ticks.scheduled().next().unwrap().due, expected + 5);
    }
    submit_command(&mut app, window, "/time set invalid");
    assert_eq!(app.world().resource::<WorldTick>().world_time(), 1050);
    for (command, feedback) in [
        ("/time query daytime", "Daytime: 1050"),
        ("/time query gametime", "World time: 1050"),
        ("/time query day", "Day: 0"),
    ] {
        submit_command(&mut app, window, command);
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0 == feedback)
        );
    }
    submit_command(&mut app, window, "/time set 18446744073709551615");
    submit_command(&mut app, window, "/time add 1");
    assert_eq!(app.world().resource::<WorldTick>().world_time(), u64::MAX);
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "Time would exceed the u64 range")
    );
}

fn parse_command(text: &str) -> Result<ChatCommand, String> {
    CommandRegistry::default().parse(text)
}

fn parse_clock(
    _: &CommandRegistry,
    args: &[&str],
) -> Result<ChatCommand, game::chat::registry::CommandParseError> {
    use game::chat::commands::TimeQuery;
    use game::chat::registry::CommandParseError;
    if !args.is_empty() {
        return Err(CommandParseError::Usage);
    }
    Ok(ChatCommand::TimeQuery(TimeQuery::Daytime))
}

#[test]
fn registry_owns_registration_and_uses_it_for_dispatch_usage_and_help() {
    let mut registry = CommandRegistry::default();
    let name = String::from("clock");
    registry
        .register(
            name.clone(),
            String::from("Read the current daytime."),
            vec![format!("/{name}")],
            parse_clock,
        )
        .unwrap();
    drop(name);
    assert_eq!(
        registry.parse("/help /clock"),
        Ok(ChatCommand::Help(Some("clock".into())))
    );
    assert_eq!(
        registry.parse("/clock"),
        parse_command("/time query daytime")
    );
    assert_eq!(registry.parse("/clock extra"), Err("Usage: /clock".into()));
    assert_eq!(
        registry.help(Some("clock")),
        ["Read the current daytime.", "/clock"]
    );
    assert_eq!(registry.help(None).last().unwrap(), "/clock");
    assert!(registry.parse("clock").is_err());
    assert!(registry.parse("//clock").is_err());
    for (name, description, usages) in [
        ("clock", "Duplicate.", vec!["/clock"]),
        ("", "Empty name.", vec!["/"]),
        ("/clock", "Slash in name.", vec!["//clock"]),
        ("two words", "Whitespace in name.", vec!["/two words"]),
        ("missing", "   ", vec!["/missing"]),
        ("missing", "No usage.", vec![]),
        ("missing", "Empty usage.", vec![""]),
        ("missing", "Wrong usage.", vec!["/other"]),
    ] {
        let before = registry.help(None);
        assert!(
            registry
                .register(name, description, usages, parse_clock)
                .is_err()
        );
        assert_eq!(registry.help(None), before);
    }
    assert_eq!(
        registry.get("clock").unwrap().description(),
        "Read the current daytime."
    );
    assert_eq!(registry.get("clock").unwrap().usages(), &["/clock"]);
}

#[test]
fn builtin_usage_errors_come_from_the_registered_help_forms() {
    let registry = CommandRegistry::default();
    for name in ["help", "time", "give", "tp", "setblock", "wireframe"] {
        let invalid = if name == "help" {
            "/help time extra".to_owned()
        } else {
            format!("/{name}")
        };
        assert_eq!(
            registry.parse(&invalid),
            Err(format!(
                "Usage: {}",
                registry.get(name).unwrap().usages().join(" | ")
            ))
        );
    }
}

#[test]
fn chat_backend_dispatches_multiple_submissions_without_ui_or_window() {
    use game::chat::ChatHistory;
    use game::chat::ChatPlugin;
    use game::chat::ChatSubmission;
    use game::world::tick::WorldTick;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<WorldChunks>()
        .init_resource::<WorldTick>()
        .add_plugins(ChatPlugin);
    app.world_mut()
        .write_message(ChatSubmission("hello".into()));
    app.world_mut()
        .write_message(ChatSubmission("/time set noon".into()));
    app.world_mut()
        .write_message(ChatSubmission("/unknown".into()));
    app.world_mut()
        .write_message(ChatSubmission("/time query".into()));
    app.update();

    assert_eq!(app.world().resource::<WorldTick>().world_time(), 6000);
    let lines: Vec<_> = app
        .world()
        .resource::<ChatHistory>()
        .messages()
        .map(|message| message.text.as_str())
        .collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], "Daytime: 6000");
    assert!(lines[1].starts_with("Unknown command:"));
    assert_eq!(lines[3], "<Player> hello");
    app.update();
    assert_eq!(app.world().resource::<ChatHistory>().messages().count(), 4);
    assert_eq!(
        app.world_mut().query::<&Node>().iter(app.world()).count(),
        0
    );
}

#[test]
fn backend_history_keeps_whole_unicode_messages_and_ages_on_world_ticks() {
    use game::chat::ChatHistory;
    use game::chat::ChatPlugin;
    use game::world::tick::WorldTick;

    let text = "世界 ".repeat(40);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<WorldChunks>()
        .init_resource::<WorldTick>()
        .add_plugins(ChatPlugin);
    app.world_mut()
        .resource_mut::<ChatHistory>()
        .push(text.clone());
    app.world_mut().resource_mut::<WorldTick>().advance(0.1);
    app.update();
    let message = app
        .world()
        .resource::<ChatHistory>()
        .messages()
        .next()
        .unwrap();
    assert_eq!(message.text, text);
    assert_eq!(message.age_ticks, 2);
    app.world_mut().resource_mut::<WorldTick>().idle();
    app.update();
    assert_eq!(
        app.world()
            .resource::<ChatHistory>()
            .messages()
            .next()
            .unwrap()
            .age_ticks,
        2
    );

    for index in 0..70 {
        app.world_mut()
            .resource_mut::<ChatHistory>()
            .push(format!("Message {index}"));
    }
    let history = app.world().resource::<ChatHistory>();
    assert_eq!(history.messages().count(), 50);
    assert_eq!(history.messages().next().unwrap().text, "Message 69");
}
