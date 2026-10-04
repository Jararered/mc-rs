//! The world list and the new world form behind the title menu's Play button.

use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;

use super::menu::MenuTextures;
use super::menu::SettingsContent;
use super::menu::button_rect;
use super::menu::menu_font;
use super::menu::spawn_button;
use super::menu::spawn_root;
use crate::app::session::WorldChoice;
use crate::app::session::WorldSession;
use crate::app::settings::Difficulty;
use crate::app::state::AppScreen;
use crate::random::parse_seed;
use crate::world::persistence::PersistenceConfig;
use crate::world::persistence::SAVES_DIRECTORY;
use crate::world::persistence::WorldSummary;
use crate::world::persistence::list_worlds;
use crate::world::tick::DAY_LENGTH;

const NAME_LIMIT: usize = 32;
const SEED_LIMIT: usize = 32;
const DEFAULT_NAME: &str = "New World";
const FIELD_WIDTH: f32 = 400.0;
const ROW_HEIGHT: f32 = 56.0;

pub struct WorldsPlugin;

impl Plugin for WorldsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldSession>()
            .init_resource::<WorldList>()
            .init_resource::<NewWorldForm>()
            .add_systems(OnEnter(AppScreen::WorldSelect), refresh_world_list)
            .add_systems(OnEnter(AppScreen::NewWorld), reset_form)
            .add_systems(
                Update,
                (handle_buttons, edit_form, refresh_form_labels)
                    .chain()
                    .run_if(on_world_screen),
            )
            .add_systems(Update, worlds_escape.run_if(on_world_screen));
    }
}

fn on_world_screen(screen: Res<State<AppScreen>>) -> bool {
    matches!(screen.get(), AppScreen::WorldSelect | AppScreen::NewWorld)
}

/// The saved worlds shown on the selection screen, newest first.
#[derive(Resource, Default)]
pub(super) struct WorldList(Vec<WorldSummary>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Seed,
}

/// What the new world screen will create.
#[derive(Resource)]
pub(super) struct NewWorldForm {
    name: String,
    seed: String,
    difficulty: Difficulty,
    focus: Field,
}

impl Default for NewWorldForm {
    fn default() -> Self {
        Self {
            name: DEFAULT_NAME.to_owned(),
            seed: String::new(),
            difficulty: Difficulty::default(),
            focus: Field::Name,
        }
    }
}

#[derive(Component, Clone, Copy)]
enum WorldsAction {
    Select(usize),
    NewWorld,
    BackToTitle,
    BackToList,
    Create,
    Focus(Field),
    Difficulty,
}

fn refresh_world_list(config: Option<Res<PersistenceConfig>>, mut list: ResMut<WorldList>) {
    let directory = config
        .as_ref()
        .map_or(Path::new(SAVES_DIRECTORY), |config| {
            config.saves_directory.as_path()
        });
    list.0 = list_worlds(directory);
}

fn reset_form(mut form: ResMut<NewWorldForm>) {
    *form = NewWorldForm::default();
}

pub(super) fn spawn_world_select(
    commands: &mut Commands,
    textures: &MenuTextures,
    list: &WorldList,
) {
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
                Text::new("Select World"),
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
                .spawn((
                    SettingsContent,
                    ScrollPosition::default(),
                    Node {
                        width: percent(100),
                        max_width: px(FIELD_WIDTH + 120.0),
                        min_height: px(0),
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                ))
                .with_children(|content| {
                    if list.0.is_empty() {
                        content.spawn((
                            Text::new("No saved worlds yet"),
                            menu_font(textures, 16.0),
                            TextColor(Color::srgb(0.7, 0.7, 0.7)),
                            Node {
                                align_self: AlignSelf::Center,
                                margin: UiRect::top(px(24)),
                                ..default()
                            },
                        ));
                    }
                    for (index, world) in list.0.iter().enumerate() {
                        spawn_world_row(content, textures, index, world);
                    }
                });
            spawn_button(
                parent,
                textures,
                "Create New World",
                WorldsAction::NewWorld,
                FIELD_WIDTH,
                None,
            );
            spawn_button(
                parent,
                textures,
                "Back",
                WorldsAction::BackToTitle,
                FIELD_WIDTH,
                None,
            );
        });
    });
}

fn spawn_world_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    textures: &MenuTextures,
    index: usize,
    world: &WorldSummary,
) {
    let manifest = &world.manifest;
    let day = manifest.world_time / DAY_LENGTH + 1;
    parent
        .spawn((
            Button,
            WorldsAction::Select(index),
            Node {
                width: percent(100),
                height: px(ROW_HEIGHT),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(4),
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
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(manifest.name.clone()),
                menu_font(textures, 16.0),
                TextColor(Color::WHITE),
                TextShadow::default(),
            ));
            row.spawn((
                Text::new(format!(
                    "Seed {}  -  Day {day}",
                    manifest.seed.cast_signed()
                )),
                menu_font(textures, 12.0),
                TextColor(Color::srgb(0.75, 0.75, 0.75)),
            ));
        });
}

pub(super) fn spawn_new_world(
    commands: &mut Commands,
    textures: &MenuTextures,
    form: &NewWorldForm,
) {
    let root = spawn_root(commands, textures, 12.0);
    commands.entity(root).with_children(|parent| {
        parent.spawn((
            Text::new("Create New World"),
            menu_font(textures, 32.0),
            TextColor(Color::WHITE),
            TextShadow::default(),
            Node {
                margin: UiRect::bottom(px(16)),
                ..default()
            },
        ));
        spawn_button(
            parent,
            textures,
            &field_text(form, Field::Name),
            WorldsAction::Focus(Field::Name),
            FIELD_WIDTH,
            None,
        );
        spawn_button(
            parent,
            textures,
            &field_text(form, Field::Seed),
            WorldsAction::Focus(Field::Seed),
            FIELD_WIDTH,
            None,
        );
        spawn_button(
            parent,
            textures,
            &difficulty_text(form.difficulty),
            WorldsAction::Difficulty,
            FIELD_WIDTH,
            None,
        );
        parent.spawn(Node {
            height: px(16),
            ..default()
        });
        spawn_button(
            parent,
            textures,
            "Create New World",
            WorldsAction::Create,
            FIELD_WIDTH,
            None,
        );
        spawn_button(
            parent,
            textures,
            "Back",
            WorldsAction::BackToList,
            FIELD_WIDTH,
            None,
        );
    });
}

fn field_text(form: &NewWorldForm, field: Field) -> String {
    let (label, value) = match field {
        Field::Name => ("World name", &form.name),
        Field::Seed => ("Seed", &form.seed),
    };
    let cursor = if form.focus == field { "_" } else { "" };
    if field == Field::Seed && value.is_empty() && form.focus != field {
        return "Seed: (random)".to_owned();
    }
    format!("{label}: {value}{cursor}")
}

fn difficulty_text(difficulty: Difficulty) -> String {
    format!(
        "Difficulty: {}",
        match difficulty {
            Difficulty::Peaceful => "Peaceful",
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
        }
    )
}

fn handle_buttons(
    mut buttons: Query<
        (
            &Interaction,
            &WorldsAction,
            &mut ImageNode,
            &mut BackgroundColor,
        ),
        Changed<Interaction>,
    >,
    list: Res<WorldList>,
    mut form: ResMut<NewWorldForm>,
    mut session: ResMut<WorldSession>,
    mut next_screen: ResMut<NextState<AppScreen>>,
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
        match *action {
            WorldsAction::Select(index) => {
                if let Some(world) = list.0.get(index)
                    && !session.is_busy()
                {
                    session.request_load(WorldChoice::Existing(world.root.clone()));
                }
            }
            WorldsAction::NewWorld => next_screen.set(AppScreen::NewWorld),
            WorldsAction::BackToTitle => next_screen.set(AppScreen::Menu),
            WorldsAction::BackToList => next_screen.set(AppScreen::WorldSelect),
            WorldsAction::Create => create_world(&form, &mut session),
            WorldsAction::Focus(field) => form.focus = field,
            WorldsAction::Difficulty => form.difficulty = form.difficulty.cycle(),
        }
        // One click acts once, even if several buttons somehow changed.
        break;
    }
}

fn create_world(form: &NewWorldForm, session: &mut WorldSession) {
    if session.is_busy() {
        return;
    }
    let name = form.name.trim();
    let seed = if form.seed.is_empty() {
        random_seed()
    } else {
        parse_seed(&form.seed)
    };
    session.request_load(WorldChoice::New {
        name: if name.is_empty() {
            DEFAULT_NAME.to_owned()
        } else {
            name.to_owned()
        },
        seed,
        difficulty: form.difficulty,
    });
}

/// A seed for a world made without one: the clock, scrambled.
fn random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    let mut value = nanos.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^= value >> 32;
    value.wrapping_mul(0xbf58_476d_1ce4_e5b9)
}

/// Typing edits the focused field; Tab moves between fields and Enter creates.
fn edit_form(
    mut keyboard: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    screen: Res<State<AppScreen>>,
    mut form: ResMut<NewWorldForm>,
    mut session: ResMut<WorldSession>,
) {
    if *screen.get() != AppScreen::NewWorld {
        keyboard.read().for_each(drop);
        return;
    }
    let modified = keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight)
        || keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight);
    for event in keyboard.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        match event.key_code {
            KeyCode::Tab => {
                form.focus = match form.focus {
                    Field::Name => Field::Seed,
                    Field::Seed => Field::Name,
                };
            }
            KeyCode::Enter | KeyCode::NumpadEnter if !event.repeat => {
                create_world(&form, &mut session);
            }
            KeyCode::Backspace => {
                let focus = form.focus;
                field_mut(&mut form, focus).pop();
            }
            _ if !modified => {
                if let Some(text) = &event.text {
                    let focus = form.focus;
                    let limit = if focus == Field::Name {
                        NAME_LIMIT
                    } else {
                        SEED_LIMIT
                    };
                    let value = field_mut(&mut form, focus);
                    for character in text.chars().filter(|c| !c.is_control()) {
                        if value.chars().count() >= limit {
                            break;
                        }
                        value.push(character);
                    }
                }
            }
            _ => {}
        }
    }
}

fn field_mut(form: &mut NewWorldForm, field: Field) -> &mut String {
    match field {
        Field::Name => &mut form.name,
        Field::Seed => &mut form.seed,
    }
}

fn refresh_form_labels(
    form: Res<NewWorldForm>,
    buttons: Query<(&WorldsAction, &Children)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    if !form.is_changed() {
        return;
    }
    for (action, children) in &buttons {
        let (value, focused) = match action {
            WorldsAction::Focus(field) => (field_text(&form, *field), form.focus == *field),
            WorldsAction::Difficulty => (difficulty_text(form.difficulty), false),
            _ => continue,
        };
        for child in children {
            if let Ok((mut text, mut color)) = texts.get_mut(*child) {
                **text = value.clone();
                color.0 = if focused {
                    Color::srgb(1.0, 1.0, 0.5)
                } else {
                    Color::WHITE
                };
            }
        }
    }
}

/// Escape steps back one screen, like the Back button.
fn worlds_escape(
    keys: Res<ButtonInput<KeyCode>>,
    screen: Res<State<AppScreen>>,
    mut next_screen: ResMut<NextState<AppScreen>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match screen.get() {
        AppScreen::WorldSelect => next_screen.set(AppScreen::Menu),
        AppScreen::NewWorld => next_screen.set(AppScreen::WorldSelect),
        _ => {}
    }
}
