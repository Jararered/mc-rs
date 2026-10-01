//! The main menu and the options screen behind it.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::math::Rect;
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;

use crate::app::settings::GameSettings;
use crate::app::settings::GraphicsQuality;
use crate::app::state::AppScreen;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_menu_textures)
            .add_systems(OnExit(AppScreen::Menu), despawn_menu)
            .add_systems(OnExit(AppScreen::Settings), despawn_menu)
            .add_systems(
                Update,
                (ensure_menu, handle_buttons, refresh_settings_labels).chain(),
            )
            .add_systems(
                Update,
                settings_escape.run_if(in_state(AppScreen::Settings)),
            );
    }
}

#[derive(Resource)]
struct MenuTextures {
    background: Handle<Image>,
    buttons: Handle<Image>,
    logo: Handle<Image>,
    font: Handle<Font>,
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component, Clone, Copy)]
enum MenuAction {
    Play,
    Settings,
    Quit,
    Back,
    RenderDistance(i32),
    Brightness(f32),
    Fov(f32),
    CloudHeight(f32),
    OldLighting,
    SmoothLighting,
    DirectionalLighting,
    WiggleLeaves,
    Graphics,
    MaxFps,
}

#[derive(Component, Clone, Copy)]
enum SettingLabel {
    RenderDistance,
    Brightness,
    Fov,
    CloudHeight,
    OldLighting,
    SmoothLighting,
    DirectionalLighting,
    WiggleLeaves,
    Graphics,
    MaxFps,
}

fn load_menu_textures(mut commands: Commands, asset_server: Res<AssetServer>) {
    let load = |path| asset_server.load(path);
    commands.insert_resource(MenuTextures {
        background: load("gui/background.png"),
        buttons: load("gui/gui.png"),
        logo: load("gui/logo.png"),
        font: asset_server.load("font/minecraft.otf"),
    });
}

fn menu_font(textures: &MenuTextures, size: f32) -> TextFont {
    TextFont::from_font_size(size)
        .with_font(textures.font.clone())
        .with_font_smoothing(FontSmoothing::None)
}

fn spawn_root(commands: &mut Commands, textures: &MenuTextures, gap: f32) -> Entity {
    commands
        .spawn((
            MenuRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(gap),
                ..default()
            },
            ImageNode::new(textures.background.clone())
                .with_color(Color::srgb(0.45, 0.45, 0.45))
                .with_mode(NodeImageMode::Tiled {
                    tile_x: true,
                    tile_y: true,
                    stretch_value: 3.0,
                }),
            BackgroundColor(Color::srgb(0.18, 0.15, 0.13)),
        ))
        .id()
}

fn ensure_menu(
    mut commands: Commands,
    screen: Res<State<AppScreen>>,
    textures: Res<MenuTextures>,
    settings: Res<GameSettings>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    if !roots.is_empty() {
        return;
    }
    match screen.get() {
        AppScreen::Menu => spawn_main_menu(&mut commands, &textures),
        AppScreen::Settings => spawn_settings_menu(&mut commands, &textures, &settings),
        AppScreen::Playing => {}
    }
}

fn spawn_main_menu(commands: &mut Commands, textures: &MenuTextures) {
    let root = spawn_root(commands, textures, 14.0);
    commands.entity(root).with_children(|parent| {
        parent.spawn((
            ImageNode::new(textures.logo.clone()).with_rect(Rect::new(0.0, 0.0, 256.0, 64.0)),
            Node {
                width: px(512),
                height: px(128),
                margin: UiRect::bottom(px(24)),
                ..default()
            },
        ));
        spawn_button(parent, &textures, "Play", MenuAction::Play, 400.0, None);
        spawn_button(
            parent,
            &textures,
            "Settings",
            MenuAction::Settings,
            400.0,
            None,
        );
        spawn_button(parent, &textures, "Quit", MenuAction::Quit, 400.0, None);
    });
}

fn spawn_settings_menu(commands: &mut Commands, textures: &MenuTextures, settings: &GameSettings) {
    let root = spawn_root(commands, textures, 6.0);
    commands.entity(root).with_children(|parent| {
        parent.spawn((
            Text::new("Settings"),
            menu_font(textures, 32.0),
            TextColor(Color::WHITE),
            TextShadow::default(),
            Node {
                margin: UiRect::bottom(px(24)),
                ..default()
            },
        ));
        spawn_stepper(
            parent,
            &textures,
            SettingLabel::RenderDistance,
            render_distance_text(&settings),
            MenuAction::RenderDistance(-1),
            MenuAction::RenderDistance(1),
        );
        spawn_stepper(
            parent,
            &textures,
            SettingLabel::Fov,
            fov_text(&settings),
            MenuAction::Fov(-5.0),
            MenuAction::Fov(5.0),
        );
        spawn_stepper(
            parent,
            &textures,
            SettingLabel::Brightness,
            brightness_text(&settings),
            MenuAction::Brightness(-50.0),
            MenuAction::Brightness(50.0),
        );
        spawn_stepper(
            parent,
            &textures,
            SettingLabel::CloudHeight,
            cloud_height_text(&settings),
            MenuAction::CloudHeight(-8.0),
            MenuAction::CloudHeight(8.0),
        );
        spawn_setting_button(
            parent,
            &textures,
            old_lighting_text(&settings),
            MenuAction::OldLighting,
            SettingLabel::OldLighting,
        );
        spawn_setting_button(
            parent,
            &textures,
            smooth_lighting_text(&settings),
            MenuAction::SmoothLighting,
            SettingLabel::SmoothLighting,
        );
        spawn_setting_button(
            parent,
            &textures,
            directional_lighting_text(&settings),
            MenuAction::DirectionalLighting,
            SettingLabel::DirectionalLighting,
        );
        spawn_setting_button(
            parent,
            &textures,
            wiggle_leaves_text(&settings),
            MenuAction::WiggleLeaves,
            SettingLabel::WiggleLeaves,
        );
        spawn_setting_button(
            parent,
            &textures,
            max_fps_text(&settings),
            MenuAction::MaxFps,
            SettingLabel::MaxFps,
        );
        spawn_setting_button(
            parent,
            &textures,
            graphics_text(&settings),
            MenuAction::Graphics,
            SettingLabel::Graphics,
        );
        parent.spawn(Node {
            height: px(16),
            ..default()
        });
        spawn_button(parent, &textures, "Back", MenuAction::Back, 520.0, None);
    });
}

fn spawn_stepper(
    parent: &mut ChildSpawnerCommands,
    textures: &MenuTextures,
    label: SettingLabel,
    value: String,
    decrease: MenuAction,
    increase: MenuAction,
) {
    parent
        .spawn(Node {
            width: px(520),
            height: px(44),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        })
        .with_children(|row| {
            spawn_button(row, textures, "-", decrease, 48.0, None);
            row.spawn((
                label,
                Text::new(value),
                menu_font(textures, 21.0),
                TextColor(Color::WHITE),
                TextShadow::default(),
            ));
            spawn_button(row, textures, "+", increase, 48.0, None);
        });
}

fn spawn_setting_button(
    parent: &mut ChildSpawnerCommands,
    textures: &MenuTextures,
    value: String,
    action: MenuAction,
    label: SettingLabel,
) {
    spawn_button(parent, textures, &value, action, 520.0, Some(label));
}

fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    textures: &MenuTextures,
    title: &str,
    action: MenuAction,
    width: f32,
    label: Option<SettingLabel>,
) -> Entity {
    let mut button = parent.spawn((
        Button,
        action,
        Node {
            width: px(width),
            height: px(40),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        ImageNode::new(textures.buttons.clone()).with_rect(button_rect(false)),
        BackgroundColor(Color::srgb(0.44, 0.44, 0.44)),
    ));
    if let Some(label) = label {
        button.insert(label);
    }
    button
        .with_child((
            Text::new(title),
            menu_font(textures, 16.0),
            TextColor(Color::WHITE),
            TextShadow::default(),
        ))
        .id()
}

fn button_rect(hovered: bool) -> Rect {
    let top = if hovered { 86.0 } else { 66.0 };
    Rect::new(0.0, top, 200.0, top + 20.0)
}

fn despawn_menu(mut commands: Commands, roots: Query<Entity, With<MenuRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

fn handle_buttons(
    mut buttons: Query<
        (
            &Interaction,
            &MenuAction,
            &mut ImageNode,
            &mut BackgroundColor,
        ),
        Changed<Interaction>,
    >,
    mut settings: ResMut<GameSettings>,
    mut next_screen: ResMut<NextState<AppScreen>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, action, mut image, mut background) in &mut buttons {
        image.rect = Some(button_rect(*interaction != Interaction::None));
        background.0 = if *interaction == Interaction::None {
            Color::srgb(0.44, 0.44, 0.44)
        } else {
            Color::srgb(0.57, 0.57, 0.72)
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            MenuAction::Play => next_screen.set(AppScreen::Playing),
            MenuAction::Settings => next_screen.set(AppScreen::Settings),
            MenuAction::Quit => {
                exit.write(AppExit::Success);
            }
            MenuAction::Back => next_screen.set(AppScreen::Menu),
            MenuAction::RenderDistance(change) => settings.change_render_distance(*change),
            MenuAction::Brightness(change) => settings.change_brightness(*change),
            MenuAction::Fov(change) => settings.change_fov(*change),
            MenuAction::CloudHeight(change) => settings.change_cloud_height(*change),
            MenuAction::OldLighting => settings.old_lighting = !settings.old_lighting,
            MenuAction::SmoothLighting => settings.smooth_lighting = !settings.smooth_lighting,
            MenuAction::DirectionalLighting => {
                settings.directional_lighting = !settings.directional_lighting;
            }
            MenuAction::WiggleLeaves => settings.wiggle_leaves = !settings.wiggle_leaves,
            MenuAction::Graphics => settings.cycle_graphics(),
            MenuAction::MaxFps => settings.cycle_max_fps(),
        }
    }
}

fn refresh_settings_labels(
    settings: Res<GameSettings>,
    labels: Query<(&SettingLabel, Entity, Option<&Children>)>,
    mut texts: Query<&mut Text>,
) {
    if !settings.is_changed() {
        return;
    }
    for (label, entity, children) in &labels {
        let value = match label {
            SettingLabel::RenderDistance => render_distance_text(&settings),
            SettingLabel::Brightness => brightness_text(&settings),
            SettingLabel::Fov => fov_text(&settings),
            SettingLabel::CloudHeight => cloud_height_text(&settings),
            SettingLabel::OldLighting => old_lighting_text(&settings),
            SettingLabel::SmoothLighting => smooth_lighting_text(&settings),
            SettingLabel::DirectionalLighting => directional_lighting_text(&settings),
            SettingLabel::WiggleLeaves => wiggle_leaves_text(&settings),
            SettingLabel::Graphics => graphics_text(&settings),
            SettingLabel::MaxFps => max_fps_text(&settings),
        };
        if let Ok(mut text) = texts.get_mut(entity) {
            **text = value;
        } else if let Some(children) = children {
            for child in children {
                if let Ok(mut text) = texts.get_mut(*child) {
                    **text = value.clone();
                    break;
                }
            }
        }
    }
}

fn render_distance_text(settings: &GameSettings) -> String {
    format!("Render distance: {} chunks", settings.render_distance)
}

fn brightness_text(settings: &GameSettings) -> String {
    format!("Ambient brightness: {:.0}", settings.brightness)
}

fn fov_text(settings: &GameSettings) -> String {
    format!("FOV: {:.0}", settings.fov)
}

fn cloud_height_text(settings: &GameSettings) -> String {
    format!("Cloud height: {:.0}", settings.cloud_height)
}

fn old_lighting_text(settings: &GameSettings) -> String {
    format!(
        "Old lighting: {}",
        if settings.old_lighting { "ON" } else { "OFF" }
    )
}

fn directional_lighting_text(settings: &GameSettings) -> String {
    format!(
        "Directional lighting: {}",
        if settings.directional_lighting {
            "ON"
        } else {
            "OFF"
        }
    )
}

fn smooth_lighting_text(settings: &GameSettings) -> String {
    format!(
        "Smooth lighting: {}",
        if settings.smooth_lighting {
            "ON"
        } else {
            "OFF"
        }
    )
}

fn graphics_text(settings: &GameSettings) -> String {
    format!(
        "Graphics: {}",
        match settings.graphics {
            GraphicsQuality::Fast => "Fast",
            GraphicsQuality::Fancy => "Fancy",
            GraphicsQuality::Ultra => "Ultra",
        }
    )
}

fn wiggle_leaves_text(settings: &GameSettings) -> String {
    format!(
        "Wiggle leaves: {}",
        if settings.wiggle_leaves { "ON" } else { "OFF" }
    )
}

fn settings_escape(keys: Res<ButtonInput<KeyCode>>, mut next_screen: ResMut<NextState<AppScreen>>) {
    if keys.just_pressed(KeyCode::Escape) {
        next_screen.set(AppScreen::Menu);
    }
}

fn max_fps_text(settings: &GameSettings) -> String {
    if settings.max_fps == 0 {
        "Max FPS: VSync".to_string()
    } else {
        format!("Max FPS: {}", settings.max_fps)
    }
}
