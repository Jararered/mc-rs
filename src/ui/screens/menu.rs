//! The main menu and the options screen behind it.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::mouse::MouseWheel;
use bevy::math::Rect;
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use bevy::window::PrimaryWindow;

use crate::app::settings::*;
use crate::ui::slider::Slider;
use crate::ui::slider::SliderDrag;
use crate::ui::slider::SliderSkin;
use crate::ui::slider::spawn_slider;
use crate::ui::slider::update_sliders;
use bevy::ui::FocusPolicy;

use crate::app::state::AppScreen;
use crate::app::state::PauseMenu;
use crate::app::state::SettingsReturn;

use super::panorama;
use super::panorama::MenuPanoramaRoot;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        panorama::plugin(app);
        app.add_plugins(super::worlds::WorldsPlugin);
        app.init_resource::<SliderDrag>()
            .init_resource::<PauseMenu>()
            .init_resource::<SettingsReturn>();
        app.add_systems(PreStartup, load_menu_textures)
            .add_systems(OnExit(AppScreen::Menu), despawn_menu)
            .add_systems(OnExit(AppScreen::Settings), despawn_menu)
            .add_systems(OnExit(AppScreen::WorldSelect), despawn_menu)
            .add_systems(OnExit(AppScreen::NewWorld), despawn_menu)
            .add_systems(
                Update,
                (
                    ensure_menu,
                    handle_buttons,
                    update_sliders,
                    apply_setting_sliders,
                    refresh_settings_labels,
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
pub(super) struct MenuTextures {
    background: Handle<Image>,
    pub(super) buttons: Handle<Image>,
    logo: Handle<Image>,
    pub(super) font: Handle<Font>,
}

#[derive(Component)]
pub(super) struct MenuRoot;

#[derive(Component, Clone, Copy)]
enum MenuAction {
    Play,
    Settings,
    Quit,
    Back,
    SmoothLighting,
    WiggleLeaves,
    Graphics,
    AntiAliasing,
    Difficulty,
    ViewBobbing,
    Fullscreen,
    Tab(SettingsTab),
}

#[derive(Component, Clone, Copy)]
pub(super) enum SettingLabel {
    RenderDistance,
    Fov,
    CloudHeight,
    SmoothLighting,
    WiggleLeaves,
    Graphics,
    AntiAliasing,
    Difficulty,
    MaxFps,
    MouseSensitivity,
    ViewBobbing,
    Fullscreen,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Video,
    Controls,
    Gameplay,
}

#[derive(Component)]
pub(super) struct SettingsContent;

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
        logo: load("title/mclogo.png"),
        font: asset_server.load("font/minecraft.otf"),
    });
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

pub(super) fn menu_font(textures: &MenuTextures, size: f32) -> TextFont {
    TextFont::from_font_size(size)
        .with_font(textures.font.clone())
        .with_font_smoothing(FontSmoothing::None)
}

pub(super) fn spawn_root(commands: &mut Commands, textures: &MenuTextures, gap: f32) -> Entity {
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
    worlds: Res<super::worlds::WorldList>,
    delete: Res<super::worlds::DeleteWorld>,
    form: Res<super::worlds::NewWorldForm>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    if !roots.is_empty() {
        return;
    }
    match screen.get() {
        AppScreen::Menu => spawn_main_menu(&mut commands, &textures),
        AppScreen::Settings => spawn_settings_menu(&mut commands, &textures, &settings),
        AppScreen::WorldSelect => {
            super::worlds::spawn_world_select(&mut commands, &textures, &worlds, &delete)
        }
        AppScreen::NewWorld => super::worlds::spawn_new_world(&mut commands, &textures, &form),
        AppScreen::Playing => {}
    }
}

fn spawn_main_menu(commands: &mut Commands, textures: &MenuTextures) {
    let root = spawn_root(commands, textures, 14.0);
    commands.entity(root).insert(MenuPanoramaRoot);
    commands.entity(root).with_children(|parent| {
        // The Beta logo atlas stores its two horizontal halves in separate rows.
        parent
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                margin: UiRect::bottom(px(24)),
                ..default()
            })
            .with_children(|logo| {
                logo.spawn((
                    ImageNode::new(textures.logo.clone())
                        .with_rect(Rect::new(0.0, 0.0, 155.0, 44.0)),
                    Node {
                        width: px(310),
                        height: px(88),
                        ..default()
                    },
                ));
                logo.spawn((
                    ImageNode::new(textures.logo.clone())
                        .with_rect(Rect::new(0.0, 45.0, 119.0, 89.0)),
                    Node {
                        width: px(238),
                        height: px(88),
                        ..default()
                    },
                ));
            });
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
    commands.entity(root).with_children(|root| {
        // Padding sits on an inner node: an image background only paints the
        // content box, so padding on the root shows as a dark border.
        root.spawn(Node {
            width: percent(100),
            height: percent(100),
            padding: UiRect::all(px(16)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(12),
            ..default()
        })
        .with_children(|parent| {
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
                    spawn_button(
                        tabs,
                        textures,
                        "Gameplay",
                        MenuAction::Tab(SettingsTab::Gameplay),
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
                            spawn_setting_slider(
                                parent,
                                &textures,
                                SettingLabel::RenderDistance,
                                render_distance_text(&settings),
                                settings,
                            );
                            spawn_setting_slider(
                                parent,
                                &textures,
                                SettingLabel::Fov,
                                fov_text(&settings),
                                settings,
                            );
                            spawn_setting_slider(
                                parent,
                                &textures,
                                SettingLabel::CloudHeight,
                                cloud_height_text(&settings),
                                settings,
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
                                wiggle_leaves_text(&settings),
                                MenuAction::WiggleLeaves,
                                SettingLabel::WiggleLeaves,
                            );
                            spawn_setting_slider(
                                parent,
                                textures,
                                SettingLabel::MaxFps,
                                max_fps_text(settings),
                                settings,
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
                                &textures,
                                anti_aliasing_text(&settings),
                                MenuAction::AntiAliasing,
                                SettingLabel::AntiAliasing,
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
                            SettingsTab::Gameplay,
                            Node {
                                width: percent(100),
                                flex_shrink: 0.0,
                                align_self: AlignSelf::Start,
                                display: Display::None,
                                ..default()
                            },
                        ))
                        .with_children(|parent| {
                            spawn_setting_button(
                                parent,
                                textures,
                                difficulty_text(settings),
                                MenuAction::Difficulty,
                                SettingLabel::Difficulty,
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
                            spawn_setting_slider(
                                parent,
                                textures,
                                SettingLabel::MouseSensitivity,
                                sensitivity_text(settings),
                                settings,
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
    });
}

/// Numeric settings bound to the reusable slider widget.
#[derive(Component, Clone, Copy)]
pub enum SettingsSlider {
    RenderDistance,
    Fov,
    CloudHeight,
    MouseSensitivity,
    MaxFps,
}

impl SettingsSlider {
    pub fn slider(self, settings: &GameSettings) -> Slider {
        match self {
            Self::RenderDistance => Slider::new(
                MIN_RENDER_DISTANCE as f32,
                MAX_RENDER_DISTANCE as f32,
                1.0,
                settings.render_distance as f32,
            ),
            Self::Fov => Slider::new(MIN_FOV, MAX_FOV, 1.0, settings.fov),
            Self::CloudHeight => Slider::new(
                MIN_CLOUD_HEIGHT,
                MAX_CLOUD_HEIGHT,
                1.0,
                settings.cloud_height,
            ),
            Self::MouseSensitivity => Slider::new(
                MIN_MOUSE_SENSITIVITY * 100.0,
                MAX_MOUSE_SENSITIVITY * 100.0,
                1.0,
                settings.mouse_sensitivity * 100.0,
            ),
            Self::MaxFps => Slider::new(
                MIN_MAX_FPS as f32,
                MAX_MAX_FPS as f32 + 1.0,
                1.0,
                if settings.max_fps == 0 {
                    MAX_MAX_FPS as f32 + 1.0
                } else {
                    settings.max_fps as f32
                },
            ),
        }
    }
}

fn spawn_setting_slider(
    parent: &mut ChildSpawnerCommands,
    textures: &MenuTextures,
    label: SettingLabel,
    value: String,
    settings: &GameSettings,
) {
    let binding = match label {
        SettingLabel::RenderDistance => SettingsSlider::RenderDistance,
        SettingLabel::Fov => SettingsSlider::Fov,
        SettingLabel::CloudHeight => SettingsSlider::CloudHeight,
        SettingLabel::MouseSensitivity => SettingsSlider::MouseSensitivity,
        SettingLabel::MaxFps => SettingsSlider::MaxFps,
        _ => unreachable!("not a numeric setting"),
    };
    let image = ImageNode::new(textures.buttons.clone())
        .with_rect(button_rect(false))
        .with_mode(NodeImageMode::Sliced(TextureSlicer {
            border: BorderRect::all(2.0),
            max_corner_scale: 2.0,
            ..default()
        }));
    let entity = spawn_slider(
        parent,
        binding.slider(settings),
        Some(SliderSkin {
            image: image.clone(),
            hovered_rect: button_rect(true),
        }),
    );
    parent
        .commands()
        .entity(entity)
        .insert((
            label,
            binding,
            image.with_rect(Rect::new(0.0, 46.0, 200.0, 66.0)),
        ))
        .with_child((
            Text::new(value),
            menu_font(textures, 16.0),
            TextColor(Color::WHITE),
            TextShadow::default(),
            TextLayout::justify(Justify::Center),
            FocusPolicy::Pass,
        ));
}

impl SettingsSlider {
    /// Apply a widget value without marking settings changed when it is identical.
    pub fn apply(self, slider: &Slider, settings: &mut GameSettings) -> bool {
        let value = slider.value();
        let mut next = settings.clone();
        match self {
            Self::RenderDistance => next.render_distance = value as i32,
            Self::Fov => next.fov = value,
            Self::CloudHeight => next.cloud_height = value,
            Self::MouseSensitivity => next.mouse_sensitivity = value / 100.0,
            Self::MaxFps => {
                next.max_fps = if value > MAX_MAX_FPS as f32 {
                    0
                } else {
                    value as u32
                }
            }
        }
        if next == *settings {
            return false;
        }
        *settings = next;
        true
    }
}

fn apply_setting_sliders(
    sliders: Query<(&SettingsSlider, &Slider), Changed<Slider>>,
    mut settings: ResMut<GameSettings>,
) {
    for (binding, slider) in &sliders {
        let mut next = settings.clone();
        if binding.apply(slider, &mut next) {
            *settings = next;
        }
    }
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

pub(super) fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    textures: &MenuTextures,
    title: &str,
    action: impl Bundle,
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

pub(super) fn button_rect(hovered: bool) -> Rect {
    let top = if hovered { 86.0 } else { 66.0 };
    Rect::new(0.0, top, 200.0, top + 20.0)
}

fn despawn_menu(
    mut commands: Commands,
    mut drag: ResMut<SliderDrag>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    drag.0 = None;
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
    mut panels: Query<(&SettingsTab, &mut Node)>,
    mut scroll: Query<&mut ScrollPosition, With<SettingsContent>>,
    mut settings: ResMut<GameSettings>,
    mut next_screen: ResMut<NextState<AppScreen>>,
    mut exit: MessageWriter<AppExit>,
    mut drag: ResMut<SliderDrag>,
    mut pause: ResMut<PauseMenu>,
    mut settings_return: ResMut<SettingsReturn>,
) {
    for (interaction, action, image, mut background) in &mut buttons {
        if let Some(mut image) = image {
            image.rect = Some(button_rect(*interaction != Interaction::None));
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
            MenuAction::Play => {
                pause.open = false;
                next_screen.set(AppScreen::WorldSelect);
            }
            MenuAction::Settings => {
                settings_return.0 = AppScreen::Menu;
                next_screen.set(AppScreen::Settings);
            }
            MenuAction::Quit => {
                exit.write(AppExit::Success);
            }
            MenuAction::Back => next_screen.set(settings_return.0),
            MenuAction::SmoothLighting => settings.smooth_lighting = !settings.smooth_lighting,
            MenuAction::WiggleLeaves => settings.wiggle_leaves = !settings.wiggle_leaves,
            MenuAction::Graphics => settings.cycle_graphics(),
            MenuAction::AntiAliasing => settings.anti_aliasing = !settings.anti_aliasing,
            MenuAction::Difficulty => settings.difficulty = settings.difficulty.cycle(),
            MenuAction::ViewBobbing => settings.view_bobbing = !settings.view_bobbing,
            MenuAction::Fullscreen => settings.fullscreen = !settings.fullscreen,
            MenuAction::Tab(selected) => {
                drag.0 = None;
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
            SettingLabel::Fov => fov_text(&settings),
            SettingLabel::CloudHeight => cloud_height_text(&settings),
            SettingLabel::SmoothLighting => smooth_lighting_text(&settings),
            SettingLabel::WiggleLeaves => wiggle_leaves_text(&settings),
            SettingLabel::Graphics => graphics_text(&settings),
            SettingLabel::AntiAliasing => anti_aliasing_text(&settings),
            SettingLabel::Difficulty => difficulty_text(&settings),
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

fn fov_text(settings: &GameSettings) -> String {
    format!("FOV: {:.0}", settings.fov)
}

fn cloud_height_text(settings: &GameSettings) -> String {
    format!("Cloud height: {:.0}", settings.cloud_height)
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

fn difficulty_text(settings: &GameSettings) -> String {
    format!(
        "Difficulty: {}",
        match settings.difficulty {
            Difficulty::Peaceful => "Peaceful",
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
        }
    )
}

fn graphics_text(settings: &GameSettings) -> String {
    format!(
        "Graphics: {}",
        match settings.graphics {
            GraphicsQuality::Fast => "Fast",
            GraphicsQuality::Fancy => "Fancy",
        }
    )
}

fn anti_aliasing_text(settings: &GameSettings) -> String {
    format!(
        "Anti-aliasing: {}",
        if settings.anti_aliasing { "4x" } else { "OFF" }
    )
}

fn wiggle_leaves_text(settings: &GameSettings) -> String {
    format!(
        "Wiggle leaves: {}",
        if settings.wiggle_leaves { "ON" } else { "OFF" }
    )
}

fn settings_escape(
    keys: Res<ButtonInput<KeyCode>>,
    settings_return: Res<SettingsReturn>,
    mut next_screen: ResMut<NextState<AppScreen>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next_screen.set(settings_return.0);
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
