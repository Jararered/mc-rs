//! The main menu and the options screen behind it.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::mouse::MouseWheel;
use bevy::math::Rect;
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use bevy::window::PrimaryWindow;

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
                (
                    ensure_menu,
                    handle_buttons,
                    refresh_settings_labels,
                    prepare_compact_buttons,
                    menu_asset_fallback,
                    highlight_tabs,
                    layout_settings,
                    scroll_settings,
                )
                    .chain()
                    .run_if(not(in_state(AppScreen::Playing))),
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
    compact_buttons: Option<Handle<Image>>,
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
    MouseSensitivity(f32),
    ViewBobbing,
    Fullscreen,
    Tab(SettingsTab),
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
    MouseSensitivity,
    ViewBobbing,
    Fullscreen,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Video,
    Controls,
}

#[derive(Component)]
struct SettingsContent;

fn layout_settings(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut panels: Query<&mut Node, With<SettingsTab>>,
) {
    let columns = if windows
        .single()
        .is_ok_and(|window| window.width() >= 1000.0)
    {
        2
    } else {
        1
    };
    for mut node in &mut panels {
        let tracks = vec![RepeatedGridTrack::flex(columns, 1.0)];
        if node.grid_template_columns != tracks {
            node.grid_template_columns = tracks;
        }
    }
}

fn scroll_settings(
    mut wheel: MessageReader<MouseWheel>,
    mut content: Query<(&mut ScrollPosition, &ComputedNode), With<SettingsContent>>,
) {
    for event in wheel.read() {
        let delta = -event.y
            * if event.unit == MouseScrollUnit::Line {
                40.0
            } else {
                1.0
            };
        for (mut scroll, node) in &mut content {
            let max =
                ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0);
            scroll.y = (scroll.y + delta).clamp(0.0, max);
        }
    }
}

fn load_menu_textures(mut commands: Commands, asset_server: Res<AssetServer>) {
    let load = |path| asset_server.load(path);
    commands.insert_resource(MenuTextures {
        background: load("gui/background.png"),
        buttons: load("gui/gui.png"),
        compact_buttons: None,
        logo: load("title/mclogo.png"),
        font: asset_server.load("font/minecraft.otf"),
    });
}

// Bevy scales nine-slice corners by the source aspect ratio. A compact
// strip made from the original button ends keeps square steppers at 2x scale.
fn prepare_compact_buttons(
    mut textures: ResMut<MenuTextures>,
    mut images: ResMut<Assets<Image>>,
    mut buttons: Query<(&Node, &Interaction, &mut ImageNode), With<Button>>,
) {
    if textures.compact_buttons.is_none() {
        let Some(source) = images.get(&textures.buttons) else {
            return;
        };
        let Some(data) = source.data.as_ref() else {
            return;
        };
        let width = source.width() as usize;
        if width < 200
            || source.height() < 106
            || data.len() != width * source.height() as usize * 4
        {
            return;
        }
        let mut compact = source.clone();
        compact.resize(bevy::render::render_resource::Extent3d {
            width: 24,
            height: 40,
            depth_or_array_layers: 1,
        });
        let target = compact.data.as_mut().expect("source has pixel data");
        for y in 0..40 {
            let source_y = if y < 20 { 66 + y } else { 86 + y - 20 };
            for x in 0..24 {
                let source_x = if x < 12 { x } else { 200 - 24 + x };
                let from = (source_y * width + source_x) * 4;
                let to = (y * 24 + x) * 4;
                target[to..to + 4].copy_from_slice(&data[from..from + 4]);
            }
        }
        textures.compact_buttons = Some(images.add(compact));
    }
    let handle = textures.compact_buttons.as_ref().unwrap();
    for (node, interaction, mut image) in &mut buttons {
        if node.width == px(48) && image.image == textures.buttons {
            image.image = handle.clone();
            image.rect = Some(compact_button_rect(*interaction != Interaction::None));
        }
    }
}

fn compact_button_rect(hovered: bool) -> Rect {
    let top = if hovered { 20.0 } else { 0.0 };
    Rect::new(0.0, top, 24.0, top + 20.0)
}

fn highlight_tabs(
    panels: Query<(&SettingsTab, &Node)>,
    buttons: Query<(&MenuAction, &Children)>,
    mut texts: Query<&mut TextColor>,
) {
    for (action, children) in &buttons {
        let MenuAction::Tab(tab) = action else {
            continue;
        };
        let selected = panels
            .iter()
            .any(|(panel, node)| panel == tab && node.display != Display::None);
        let color = if selected {
            Color::srgb(1.0, 1.0, 0.5)
        } else {
            Color::WHITE
        };
        for child in children {
            if let Ok(mut text) = texts.get_mut(*child) {
                text.set_if_neq(TextColor(color));
            }
        }
    }
}

// Reference art is optional; failed loads must not hide labels or controls.
fn menu_asset_fallback(
    mut commands: Commands,
    assets: Res<AssetServer>,
    textures: Res<MenuTextures>,
    mut fonts: Query<&mut TextFont>,
    images: Query<(Entity, &ImageNode)>,
) {
    if matches!(
        assets.load_state(textures.font.id()),
        bevy::asset::LoadState::Failed(_)
    ) {
        for mut font in &mut fonts {
            if font.font == bevy::text::FontSource::Handle(textures.font.clone()) {
                font.font = TextFont::default().font;
            }
        }
    }
    for (entity, image) in &images {
        if image.image != textures.background
            && image.image != textures.buttons
            && image.image != textures.logo
        {
            continue;
        }
        if matches!(
            assets.load_state(image.image.id()),
            bevy::asset::LoadState::Failed(_)
        ) {
            commands.entity(entity).remove::<ImageNode>();
        }
    }
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
            ImageNode::new(textures.logo.clone()).with_rect(Rect::new(0.0, 0.0, 155.0, 89.0)),
            Node {
                width: px(310),
                height: px(178),
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
    let root = spawn_root(commands, textures, 12.0);
    commands.entity(root).insert(Node {
        width: percent(100),
        height: percent(100),
        padding: UiRect::all(px(16)),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(12),
        ..default()
    });
    commands.entity(root).with_children(|parent| {
        parent.spawn((
            Text::new("Settings"),
            menu_font(textures, 32.0),
            TextColor(Color::WHITE),
            TextShadow::default(),
            Node {
                flex_shrink: 0.0,
                margin: UiRect::bottom(px(8)),
                ..default()
            },
        ));
        parent
            .spawn(Node {
                flex_shrink: 0.0,
                column_gap: px(12),
                ..default()
            })
            .with_children(|tabs| {
                spawn_button(
                    tabs,
                    textures,
                    "Video",
                    MenuAction::Tab(SettingsTab::Video),
                    160.0,
                    None,
                );
                spawn_button(
                    tabs,
                    textures,
                    "Controls",
                    MenuAction::Tab(SettingsTab::Controls),
                    160.0,
                    None,
                );
            });
        parent
            .spawn((
                SettingsContent,
                ScrollPosition::default(),
                Node {
                    width: percent(100),
                    max_width: px(1080),
                    min_height: px(0),
                    flex_grow: 1.0,
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
            ))
            .with_children(|content| {
                content
                    .spawn((
                        SettingsTab::Video,
                        Node {
                            width: percent(100),
                            flex_shrink: 0.0,
                            align_self: AlignSelf::Start,
                            display: Display::Grid,
                            grid_template_columns: vec![RepeatedGridTrack::flex(1, 1.0)],
                            row_gap: px(8),
                            column_gap: px(16),
                            align_items: AlignItems::Start,
                            ..default()
                        },
                    ))
                    .with_children(|parent| {
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
                        spawn_setting_button(
                            parent,
                            textures,
                            fullscreen_text(settings),
                            MenuAction::Fullscreen,
                            SettingLabel::Fullscreen,
                        );
                    });
                content
                    .spawn((
                        SettingsTab::Controls,
                        Node {
                            width: percent(100),
                            flex_shrink: 0.0,
                            align_self: AlignSelf::Start,
                            display: Display::None,
                            grid_template_columns: vec![RepeatedGridTrack::flex(1, 1.0)],
                            row_gap: px(8),
                            column_gap: px(16),
                            ..default()
                        },
                    ))
                    .with_children(|parent| {
                        spawn_stepper(
                            parent,
                            textures,
                            SettingLabel::MouseSensitivity,
                            sensitivity_text(settings),
                            MenuAction::MouseSensitivity(-0.1),
                            MenuAction::MouseSensitivity(0.1),
                        );
                        spawn_setting_button(
                            parent,
                            textures,
                            view_bobbing_text(settings),
                            MenuAction::ViewBobbing,
                            SettingLabel::ViewBobbing,
                        );
                    });
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
            width: percent(100),
            min_width: px(0),
            flex_shrink: 0.0,
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
                menu_font(textures, 16.0),
                TextLayout::justify(Justify::Center),
                Node {
                    flex_grow: 1.0,
                    min_width: px(0),
                    margin: UiRect::horizontal(px(8)),
                    ..default()
                },
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
            width: if width == 520.0 {
                percent(100)
            } else {
                px(width)
            },
            max_width: px(width),
            flex_shrink: 0.0,
            height: px(40),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        ImageNode::new(textures.buttons.clone())
            .with_rect(button_rect(false))
            .with_mode(NodeImageMode::Sliced(TextureSlicer {
                border: BorderRect::all(2.0),
                max_corner_scale: 2.0,
                ..default()
            })),
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
            Option<&mut ImageNode>,
            &mut BackgroundColor,
        ),
        Changed<Interaction>,
    >,
    textures: Res<MenuTextures>,
    mut panels: Query<(&SettingsTab, &mut Node)>,
    mut scroll: Query<&mut ScrollPosition, With<SettingsContent>>,
    mut settings: ResMut<GameSettings>,
    mut next_screen: ResMut<NextState<AppScreen>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, action, image, mut background) in &mut buttons {
        if let Some(mut image) = image {
            image.rect = Some(if textures.compact_buttons.as_ref() == Some(&image.image) {
                compact_button_rect(*interaction != Interaction::None)
            } else {
                button_rect(*interaction != Interaction::None)
            });
        }
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
            MenuAction::MouseSensitivity(change) => settings.change_mouse_sensitivity(*change),
            MenuAction::ViewBobbing => settings.view_bobbing = !settings.view_bobbing,
            MenuAction::Fullscreen => settings.fullscreen = !settings.fullscreen,
            MenuAction::Tab(selected) => {
                for (tab, mut node) in &mut panels {
                    node.display = if tab == selected {
                        Display::Grid
                    } else {
                        Display::None
                    };
                }
                for mut position in &mut scroll {
                    position.0 = Vec2::ZERO;
                }
            }
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
            SettingLabel::MouseSensitivity => sensitivity_text(&settings),
            SettingLabel::ViewBobbing => view_bobbing_text(&settings),
            SettingLabel::Fullscreen => fullscreen_text(&settings),
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

fn sensitivity_text(settings: &GameSettings) -> String {
    format!(
        "Mouse sensitivity: {:.0}%",
        settings.mouse_sensitivity * 100.0
    )
}
fn view_bobbing_text(settings: &GameSettings) -> String {
    format!(
        "View bobbing: {}",
        if settings.view_bobbing { "ON" } else { "OFF" }
    )
}
fn fullscreen_text(settings: &GameSettings) -> String {
    format!(
        "Fullscreen: {}",
        if settings.fullscreen { "ON" } else { "OFF" }
    )
}
