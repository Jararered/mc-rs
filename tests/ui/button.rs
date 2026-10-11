use bevy::camera::NormalizedRenderTarget;
use bevy::picking::backend::HitData;
use bevy::picking::events::Pointer;
use bevy::picking::events::PointerPress;
use bevy::picking::pointer::Location;
use bevy::picking::pointer::PointerButton;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::window::WindowRef;
use game::ui::Button;
use game::ui::ButtonPlugin;

fn press(app: &mut App, entity: Entity, button: PointerButton) {
    // Pointer events bubble through `ChildOf` and end at the window.
    let window = app.world_mut().spawn(Window::default()).id();
    let target = NormalizedRenderTarget::Window(WindowRef::Entity(window).normalize(None).unwrap());
    let location = Location {
        target,
        position: Vec2::ZERO,
    };
    app.world_mut().trigger(PointerPress {
        entity,
        pointer: Pointer::new(PointerId::Mouse, location),
        button,
        hit: HitData::new(window, 0.0, None, None),
        count: 1,
    });
    app.world_mut().flush();
}

fn button_app() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.init_resource::<ButtonInput<MouseButton>>()
        .add_plugins(ButtonPlugin);
    let button = app.world_mut().spawn(Button).id();
    let label = app
        .world_mut()
        .spawn((Node::default(), ChildOf(button)))
        .id();
    (app, button, label)
}

#[test]
fn a_left_press_on_a_buttons_child_presses_the_button_until_the_mouse_lets_go() {
    let (mut app, button, label) = button_app();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    press(&mut app, label, PointerButton::Primary);
    assert!(app.world().get::<Pressed>(button).is_some());
    assert!(app.world().get::<Pressed>(label).is_none());

    app.update();
    assert!(app.world().get::<Pressed>(button).is_some());

    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();
    assert!(app.world().get::<Pressed>(button).is_none());
}

#[test]
fn a_right_press_does_not_press_a_button() {
    let (mut app, button, _) = button_app();
    press(&mut app, button, PointerButton::Secondary);
    assert!(app.world().get::<Pressed>(button).is_none());
}

/// The real path: a cursor over a laid-out button, through Bevy's UI picking
/// backend, to `Hovered` and `Pressed`.
#[test]
fn picking_hovers_and_presses_a_button_under_the_cursor() {
    use bevy::input::ButtonState;
    use bevy::input::mouse::MouseButtonInput;
    use bevy::picking::hover::Hovered;
    use bevy::window::CursorMoved;
    use bevy::window::PrimaryWindow;
    use bevy::window::WindowEvent;

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        bevy::input::InputPlugin,
        bevy::window::WindowPlugin::default(),
        bevy::transform::TransformPlugin,
        bevy::image::ImagePlugin::default(),
        bevy::camera::CameraPlugin,
        bevy::text::TextPlugin,
        bevy::picking::DefaultPickingPlugins,
        bevy::ui::UiPlugin,
        ButtonPlugin,
    ))
    .add_plugins(bevy::mesh::MeshPlugin)
    .init_asset::<TextureAtlasLayout>();
    app.world_mut().spawn(Camera2d);
    let button = app
        .world_mut()
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: px(20),
                top: px(20),
                width: px(200),
                height: px(40),
                ..default()
            },
        ))
        .id();
    let label = app
        .world_mut()
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ChildOf(button),
        ))
        .id();
    let other = app
        .world_mut()
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: px(20),
                top: px(100),
                width: px(200),
                height: px(40),
                ..default()
            },
        ))
        .id();
    for _ in 0..3 {
        app.update();
    }

    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .unwrap();
    let position = Vec2::new(60.0, 40.0);
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(position));
    // winit writes each input twice: by its own type, and as a `WindowEvent`,
    // which is what picking reads.
    let moved = CursorMoved {
        window,
        position,
        delta: None,
    };
    app.world_mut().write_message(moved.clone());
    app.world_mut()
        .write_message(WindowEvent::CursorMoved(moved));
    for _ in 0..3 {
        app.update();
    }
    assert!(app.world().get::<Hovered>(button).unwrap().get());
    assert!(!app.world().get::<Hovered>(other).unwrap().get());
    assert!(app.world().get::<Pressed>(button).is_none());

    let input = MouseButtonInput {
        button: MouseButton::Left,
        state: ButtonState::Pressed,
        window,
    };
    app.world_mut().write_message(input);
    app.world_mut()
        .write_message(WindowEvent::MouseButtonInput(input));
    app.update();
    assert!(app.world().get::<Pressed>(button).is_some());
    assert!(app.world().get::<Pressed>(label).is_none());
    assert!(app.world().get::<Pressed>(other).is_none());

    let input = MouseButtonInput {
        button: MouseButton::Left,
        state: ButtonState::Released,
        window,
    };
    app.world_mut().write_message(input);
    app.world_mut()
        .write_message(WindowEvent::MouseButtonInput(input));
    app.update();
    assert!(app.world().get::<Pressed>(button).is_none());
}
