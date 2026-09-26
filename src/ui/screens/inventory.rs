use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::text::Justify;
use bevy::text::LineHeight;
use bevy::text::TextLayout;
use bevy::ui::RelativeCursorPosition;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::app::state::AppScreen;
use crate::block::id::Id;
use crate::crafting::CraftingGrid;
use crate::crafting::beta_recipe_book;
use crate::entity::drops::items::spawn_thrown_item;
use crate::inventory::DragPlace;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::SlotId;
use crate::inventory::chest_drag_place;
use crate::inventory::chest_slot_accepts_drag;
use crate::inventory::collect_matching_stacks;
use crate::inventory::drag_place;
use crate::inventory::hotbar_key_swap;
use crate::inventory::hotbar_key_swap_chest;
use crate::inventory::preview_chest_drag_place;
use crate::inventory::preview_drag_place;
use crate::inventory::quick_move_drag_slot;
use crate::inventory::shift_click_chest_slot;
use crate::inventory::shift_click_furnace_slot;
use crate::inventory::shift_click_slot;
use crate::inventory::slot_accepts_drag;
use crate::inventory::sort_container_slots;
use crate::inventory::sort_main_inventory;
use crate::item::ItemData;
use crate::item::ItemStack;
use crate::player::Player;
use crate::random::ItemRng;
use crate::ui::icons::blocks::BlockIcons;
use crate::ui::icons::overlay::GUI_SCALE;
use crate::ui::icons::overlay::UiFont;
use crate::ui::icons::overlay::count_frame;
use crate::ui::icons::overlay::count_label;
use crate::ui::icons::overlay::count_line_height;
use crate::ui::icons::overlay::count_shadow;
use crate::ui::icons::overlay::count_text_font;
use crate::ui::icons::overlay::durability_track;
use crate::ui::icons::overlay::icon_size;
use crate::ui::icons::overlay::place_stack_label;
use crate::world::chunk::ChestGroup;
use crate::world::chunk::ChunkPos;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;

const SCALE: f32 = GUI_SCALE;
const SLOT_SIZE: f32 = 16.0;
const SLOT_STEP: f32 = 18.0;
const CHEST_HALF_SLOTS: usize = 27;
const DOUBLE_CHEST_SLOTS: usize = CHEST_HALF_SLOTS * 2;

pub struct InventoryGuiPlugin;

impl Plugin for InventoryGuiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryScreen>()
            .init_resource::<WorkbenchUiSession>()
            .init_resource::<SlotDrag>()
            .init_resource::<LastInventoryClick>()
            .add_systems(PreStartup, (load_texture, crate::ui::icons::blocks::setup))
            .add_systems(Update, crate::ui::icons::blocks::build.before(refresh))
            .add_systems(
                Update,
                (
                    validate_workbench,
                    validate_furnace,
                    validate_chest,
                    toggle,
                    handle_slots,
                    highlight_slots,
                    refresh,
                )
                    .chain()
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_systems(OnExit(AppScreen::Playing), close);
    }
}

fn validate_workbench(
    mut commands: Commands,
    mut screen: ResMut<InventoryScreen>,
    mut session: ResMut<WorkbenchUiSession>,
    chunks: Res<WorldChunks>,
    player_transform: Query<&Transform, With<Player>>,
    mut player: Query<(&mut Hotbar, &mut Inventory), With<Player>>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut item_rng: Local<ItemRng>,
) {
    if !screen.open || !screen.workbench {
        return;
    }
    let Some((x, y, z)) = session.position else {
        return;
    };
    let Ok(transform) = player_transform.single() else {
        return;
    };
    let player_position = transform.translation;
    let dx = player_position.x - (x as f32 + 0.5);
    let dy = player_position.y - (y as f32 + 0.5);
    let dz = player_position.z - (z as f32 + 0.5);
    if chunks.block_at(x, y, z) == Some(Id::CraftingTable) && dx * dx + dy * dy + dz * dz <= 64.0 {
        return;
    }
    if let Ok((mut hotbar, mut inventory)) = player.single_mut() {
        close_crafting_interface(
            &mut commands,
            transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut session,
        );
    }
    screen.open = false;
    screen.workbench = false;
    session.position = None;
    for root in &roots {
        commands.entity(root).despawn();
    }
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
}

fn validate_furnace(
    mut commands: Commands,
    mut screen: ResMut<InventoryScreen>,
    chunks: Res<WorldChunks>,
    player_transform: Query<&Transform, With<Player>>,
    mut player: Query<(&mut Hotbar, &mut Inventory), With<Player>>,
    mut workbench: ResMut<WorkbenchUiSession>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut item_rng: Local<ItemRng>,
) {
    if !screen.open || !screen.furnace {
        return;
    }
    let Some((x, y, z)) = screen.furnace_position else {
        return;
    };
    let Ok(transform) = player_transform.single() else {
        return;
    };
    let delta = transform.translation - Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
    let block = chunks.block_at(x, y, z);
    if block.is_some_and(Id::is_furnace) && delta.length_squared() <= 64.0 {
        return;
    }
    screen.open = false;
    screen.furnace = false;
    screen.furnace_position = None;
    if let Ok((mut hotbar, mut inventory)) = player.single_mut() {
        close_crafting_interface(
            &mut commands,
            transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
}

fn validate_chest(
    mut commands: Commands,
    mut screen: ResMut<InventoryScreen>,
    chunks: Res<WorldChunks>,
    player_transform: Query<&Transform, With<Player>>,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>,
    mut workbench: ResMut<WorkbenchUiSession>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut item_rng: Local<ItemRng>,
) {
    if !screen.open || !screen.chest {
        return;
    }
    let Some((x, y, z)) = screen.chest_position else {
        return;
    };
    let Ok(transform) = player_transform.single() else {
        return;
    };
    let delta = transform.translation - Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
    if chunks.chest_group_at(x, y, z) == screen.chest_group && delta.length_squared() <= 64.0 {
        return;
    }
    screen.open = false;
    screen.chest = false;
    screen.chest_position = None;
    screen.chest_group = None;
    if let Ok((player_transform, mut hotbar, mut inventory)) = player.single_mut() {
        close_crafting_interface(
            &mut commands,
            player_transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
}

#[derive(Resource, Default)]
pub(crate) struct InventoryScreen {
    pub open: bool,
    pub workbench: bool,
    pub furnace: bool,
    pub furnace_position: Option<(i32, i32, i32)>,
    pub chest: bool,
    pub chest_position: Option<(i32, i32, i32)>,
    pub chest_group: Option<ChestGroup>,
}
#[derive(Resource)]
pub(crate) struct WorkbenchUiSession {
    pub grid: CraftingGrid,
    pub position: Option<(i32, i32, i32)>,
}

impl Default for WorkbenchUiSession {
    fn default() -> Self {
        Self {
            grid: CraftingGrid::workbench(),
            position: None,
        }
    }
}
#[derive(Resource)]
struct InventoryTexture {
    background: Handle<Image>,
    crafting: Handle<Image>,
    furnace: Handle<Image>,
    container: Handle<Image>,
}
#[derive(Component)]
struct InventoryRoot;
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Hotbar(usize),
    Main(usize),
    Craft(usize),
    CraftResult,
    Workbench(usize),
    Chest(usize),
    Furnace(usize),
    Armor(usize),
}
#[derive(Component)]
struct SlotLabel(Slot);
/// GuiContainer draws ARGB `0x80FFFFFF` over the hovered 16×16 slot.
#[derive(Component)]
struct SlotHighlight;
#[derive(Component)]
struct SlotIcon(Slot);
#[derive(Component)]
struct SlotDurability(Slot, bool);
#[derive(Component)]
struct CarriedLabel;
#[derive(Component)]
struct CarriedIcon;
#[derive(Component)]
struct FurnaceProgress(bool);

const HOTBAR_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

#[derive(Resource, Default)]
struct SlotDrag {
    button: Option<MouseButton>,
    origin: Option<Slot>,
    slots: Vec<Slot>,
    quick_move: bool,
    quick_move_visited: Vec<SlotId>,
}

#[derive(Resource, Default)]
struct LastInventoryClick {
    at: Option<f64>,
    slot: Option<Slot>,
    container: u8,
}

const DOUBLE_CLICK_SECONDS: f64 = 0.35;

fn is_double_click(
    last: &mut LastInventoryClick,
    screen: &InventoryScreen,
    slot: Slot,
    now: f64,
) -> bool {
    let container = u8::from(screen.chest)
        | (u8::from(screen.furnace) << 1)
        | (u8::from(screen.workbench) << 2);
    let double = last.slot == Some(slot)
        && last.container == container
        && last.at.is_some_and(|at| now - at <= DOUBLE_CLICK_SECONDS);
    last.at = Some(now);
    last.slot = Some(slot);
    last.container = container;
    double
}

fn load_texture(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(InventoryTexture {
        background: assets.load("gui/inventory.png"),
        crafting: assets.load("gui/crafting.png"),
        furnace: assets.load("gui/furnace.png"),
        container: assets.load("gui/container.png"),
    });
}

fn toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut screen: ResMut<InventoryScreen>,
    mut workbench: ResMut<WorkbenchUiSession>,
    texture: Res<InventoryTexture>,
    icons: Res<BlockIcons>,
    font: Res<UiFont>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut item_rng: Local<ItemRng>,
) {
    if screen.open && roots.is_empty() {
        let background = if screen.furnace {
            &texture.furnace
        } else if screen.workbench {
            &texture.crafting
        } else {
            &texture.background
        };
        spawn(
            &mut commands,
            background,
            &icons.image,
            &font.minecraft,
            screen.workbench,
            screen.furnace,
            screen
                .chest_group
                .map_or(0, ChestGroup::slot_count)
                .div_ceil(9),
            &texture.container,
        );
        if let Ok((_, mut cursor)) = windows.single_mut() {
            cursor.visible = true;
            cursor.grab_mode = CursorGrabMode::None;
        }
        return;
    }
    if !keys.just_pressed(KeyCode::KeyE) && !(screen.open && keys.just_pressed(KeyCode::Escape)) {
        return;
    }
    screen.open = !screen.open;
    if !screen.open {
        if let Ok((transform, mut hotbar, mut inventory)) = player.single_mut() {
            close_crafting_interface(
                &mut commands,
                transform,
                &mut item_rng,
                &mut hotbar,
                &mut inventory,
                &mut workbench,
            );
        }
        screen.workbench = false;
        screen.furnace = false;
        screen.furnace_position = None;
        screen.chest = false;
        screen.chest_position = None;
        screen.chest_group = None;
    }
    if let Ok((window, mut cursor)) = windows.single_mut() {
        cursor.visible = screen.open;
        cursor.grab_mode = if screen.open {
            CursorGrabMode::None
        } else if window.focused {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        };
    }
    if screen.open {
        let background = if screen.furnace {
            &texture.furnace
        } else if screen.workbench {
            &texture.crafting
        } else {
            &texture.background
        };
        spawn(
            &mut commands,
            background,
            &icons.image,
            &font.minecraft,
            screen.workbench,
            screen.furnace,
            screen
                .chest_group
                .map_or(0, ChestGroup::slot_count)
                .div_ceil(9),
            &texture.container,
        );
    } else {
        for root in &roots {
            commands.entity(root).despawn();
        }
    }
}

fn close(
    mut commands: Commands,
    mut screen: ResMut<InventoryScreen>,
    mut workbench: ResMut<WorkbenchUiSession>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>,
    mut item_rng: Local<ItemRng>,
) {
    if let Ok((transform, mut hotbar, mut inventory)) = player.single_mut() {
        close_crafting_interface(
            &mut commands,
            transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
    }
    screen.open = false;
    screen.workbench = false;
    screen.furnace = false;
    screen.furnace_position = None;
    screen.chest = false;
    screen.chest_position = None;
    screen.chest_group = None;
    for root in &roots {
        commands.entity(root).despawn();
    }
}

pub(crate) fn close_crafting_interface(
    commands: &mut Commands,
    player: &Transform,
    rng: &mut ItemRng,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut WorkbenchUiSession,
) {
    let mut stacks: Vec<ItemStack> = inventory
        .crafting
        .iter_mut()
        .filter_map(Option::take)
        .collect();
    stacks.extend(workbench.grid.drain());
    for stack in stacks {
        return_or_drop(commands, rng, player, hotbar, inventory, stack);
    }
    if let Some(stack) = inventory.carried.take() {
        return_or_drop(commands, rng, player, hotbar, inventory, stack);
    }
    // A closed interface must never leave a reusable session holding inputs;
    // the next workbench always starts with a fresh 3×3 grid.
    workbench.grid = CraftingGrid::workbench();
    workbench.position = None;
}

fn return_or_drop(
    commands: &mut Commands,
    rng: &mut ItemRng,
    player: &Transform,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    stack: ItemStack,
) {
    if let Some(remainder) = inventory.insert(hotbar, stack) {
        spawn_thrown_item(commands, rng, player, *player.forward(), remainder);
    }
}

fn spawn(
    commands: &mut Commands,
    texture: &Handle<Image>,
    icons: &Handle<Image>,
    font: &Handle<Font>,
    workbench: bool,
    furnace: bool,
    chest_rows: usize,
    container: &Handle<Image>,
) {
    let chest = chest_rows > 0;
    let panel_height = if chest {
        114.0 + chest_rows as f32 * 18.0
    } else {
        166.0
    };
    commands
        .spawn((
            InventoryRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Relative,
                    width: px(176.0 * SCALE),
                    height: px(panel_height * SCALE),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|panel| {
                if chest {
                    let chest_top_height = chest_rows as f32 * 18.0 + 17.0;
                    let player_top = 18.0 + chest_rows as f32 * 18.0 + 13.0;
                    let hotbar_top = player_top + 58.0;
                    panel.spawn((
                        ImageNode::new(container.clone()).with_rect(Rect::new(
                            0.0,
                            0.0,
                            176.0,
                            chest_top_height,
                        )),
                        Pickable::IGNORE,
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(0.0),
                            width: px(176.0 * SCALE),
                            height: px(chest_top_height * SCALE),
                            ..default()
                        },
                    ));
                    panel.spawn((
                        ImageNode::new(container.clone())
                            .with_rect(Rect::new(0.0, 126.0, 176.0, 222.0)),
                        Pickable::IGNORE,
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(chest_top_height * SCALE),
                            width: px(176.0 * SCALE),
                            height: px(96.0 * SCALE),
                            ..default()
                        },
                    ));
                    chest_panel_label(
                        panel,
                        font,
                        if chest_rows == 3 {
                            "Chest"
                        } else {
                            "Large chest"
                        },
                        8.0,
                        6.0,
                    );
                    chest_panel_label(panel, font, "Inventory", 8.0, chest_top_height + 3.0);
                    for index in 0..chest_rows * 9 {
                        let x = 8.0 + (index % 9) as f32 * SLOT_STEP;
                        let y = 18.0 + (index / 9) as f32 * SLOT_STEP;
                        slot(panel, icons, font, Slot::Chest(index), x, y);
                    }
                    for index in 0..27 {
                        let x = 8.0 + (index % 9) as f32 * SLOT_STEP;
                        let y = player_top + (index / 9) as f32 * SLOT_STEP;
                        slot(panel, icons, font, Slot::Main(index), x, y);
                    }
                    for index in 0..9 {
                        let x = 8.0 + index as f32 * SLOT_STEP;
                        slot(panel, icons, font, Slot::Hotbar(index), x, hotbar_top);
                    }
                } else {
                    panel.spawn((
                        ImageNode::new(texture.clone())
                            .with_rect(Rect::new(0.0, 0.0, 176.0, 166.0)),
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(0.0),
                            width: px(176.0 * SCALE),
                            height: px(166.0 * SCALE),
                            ..default()
                        },
                    ));
                    for index in 0..27 {
                        slot(
                            panel,
                            icons,
                            font,
                            Slot::Main(index),
                            8.0 + (index % 9) as f32 * SLOT_STEP,
                            84.0 + (index / 9) as f32 * SLOT_STEP,
                        );
                    }
                    for index in 0..9 {
                        slot(
                            panel,
                            icons,
                            font,
                            Slot::Hotbar(index),
                            8.0 + index as f32 * SLOT_STEP,
                            142.0,
                        );
                    }
                    if furnace {
                        slot(panel, icons, font, Slot::Furnace(0), 56.0, 17.0);
                        slot(panel, icons, font, Slot::Furnace(1), 56.0, 53.0);
                        slot(panel, icons, font, Slot::Furnace(2), 116.0, 35.0);
                        panel.spawn((
                            FurnaceProgress(false),
                            Pickable::IGNORE,
                            Visibility::Hidden,
                            ImageNode::new(texture.clone())
                                .with_rect(Rect::new(176.0, 0.0, 190.0, 14.0)),
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(56.0 * SCALE),
                                top: px(50.0 * SCALE),
                                width: px(14.0 * SCALE),
                                height: px(0.0),
                                ..default()
                            },
                        ));
                        panel.spawn((
                            FurnaceProgress(true),
                            Pickable::IGNORE,
                            Visibility::Hidden,
                            ImageNode::new(texture.clone())
                                .with_rect(Rect::new(176.0, 14.0, 176.0, 30.0)),
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(79.0 * SCALE),
                                top: px(34.0 * SCALE),
                                width: px(24.0 * SCALE),
                                height: px(16.0 * SCALE),
                                ..default()
                            },
                        ));
                    } else if workbench {
                        for index in 0..9 {
                            slot(
                                panel,
                                icons,
                                font,
                                Slot::Workbench(index),
                                30.0 + (index % 3) as f32 * SLOT_STEP,
                                17.0 + (index / 3) as f32 * SLOT_STEP,
                            );
                        }
                        slot(panel, icons, font, Slot::CraftResult, 124.0, 35.0);
                    } else {
                        for index in 0..4 {
                            slot(
                                panel,
                                icons,
                                font,
                                Slot::Craft(index),
                                88.0 + (index % 2) as f32 * SLOT_STEP,
                                26.0 + (index / 2) as f32 * SLOT_STEP,
                            );
                        }
                        slot(panel, icons, font, Slot::CraftResult, 144.0, 36.0);
                    }
                    for index in 0..4 {
                        slot(
                            panel,
                            icons,
                            font,
                            Slot::Armor(index),
                            8.0,
                            8.0 + index as f32 * SLOT_STEP,
                        );
                    }
                }
                let carried = count_frame(0.0, 0.0);
                panel.spawn((
                    CarriedIcon,
                    Pickable::IGNORE,
                    Visibility::Hidden,
                    ImageNode::new(icons.clone()),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(0),
                        width: px(icon_size()),
                        height: px(icon_size()),
                        ..default()
                    },
                ));
                panel.spawn((
                    CarriedLabel,
                    Pickable::IGNORE,
                    Text::new(""),
                    count_text_font(font),
                    TextLayout::justify(Justify::Right),
                    count_line_height(),
                    TextColor(Color::WHITE),
                    count_shadow(),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(carried.left),
                        top: px(carried.top),
                        width: px(carried.width),
                        height: px(carried.height),
                        ..default()
                    },
                ));
            });
        });
}

fn chest_panel_label(
    panel: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    label: &str,
    x: f32,
    y: f32,
) {
    panel.spawn((
        Text::new(label),
        TextFont {
            font: FontSource::Handle(font.clone()),
            font_size: FontSize::Px(8.0 * SCALE),
            ..default()
        },
        TextColor(Color::srgb_u8(64, 64, 64)),
        Pickable::IGNORE,
        Node {
            position_type: PositionType::Absolute,
            left: px(x * SCALE),
            top: px(y * SCALE),
            ..default()
        },
    ));
}

fn slot(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    icons: &Handle<Image>,
    font: &Handle<Font>,
    id: Slot,
    x: f32,
    y: f32,
) {
    parent
        .spawn((
            Button,
            id,
            RelativeCursorPosition::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(x * SCALE),
                top: px(y * SCALE),
                width: px(SLOT_SIZE * SCALE),
                height: px(SLOT_SIZE * SCALE),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|button| {
            button.spawn((
                SlotIcon(id),
                Pickable::IGNORE,
                Visibility::Hidden,
                ImageNode::new(icons.clone()),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                    width: px(icon_size()),
                    height: px(icon_size()),
                    ..default()
                },
            ));
            for foreground in [false, true] {
                let (bar_left, bar_top, bar_width, bar_height) =
                    durability_track(0.0, 0.0, foreground);
                button.spawn((
                    SlotDurability(id, foreground),
                    Pickable::IGNORE,
                    Visibility::Hidden,
                    BackgroundColor(Color::BLACK),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(bar_left),
                        top: px(bar_top),
                        width: px(bar_width),
                        height: px(bar_height),
                        ..default()
                    },
                ));
            }
            let frame = count_frame(0.0, 0.0);
            button.spawn((
                SlotLabel(id),
                Pickable::IGNORE,
                Text::new(""),
                count_text_font(font),
                TextLayout::justify(Justify::Right),
                count_line_height(),
                TextColor(Color::WHITE),
                count_shadow(),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(frame.left),
                    top: px(frame.top),
                    width: px(frame.width),
                    height: px(frame.height),
                    ..default()
                },
            ));
            button.spawn((
                SlotHighlight,
                Pickable::IGNORE,
                Visibility::Hidden,
                BackgroundColor(Color::srgba_u8(255, 255, 255, 128)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                    width: percent(100.0),
                    height: percent(100.0),
                    ..default()
                },
            ));
        });
}

fn click_slot(target: &mut Option<ItemStack>, carried: &mut Option<ItemStack>, right: bool) {
    if right {
        match (*carried, *target) {
            (None, Some(stack)) => {
                let take = (stack.count() + 1) / 2;
                *carried = ItemStack::with_data(stack.item(), take, stack.data()).ok();
                *target =
                    ItemStack::with_data(stack.item(), stack.count() - take, stack.data()).ok();
            }
            (Some(stack), None) => {
                *target = ItemStack::with_data(stack.item(), 1, stack.data()).ok();
                *carried = ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
            }
            (Some(stack), Some(existing))
                if stack.item() == existing.item()
                    && stack.data() == existing.data()
                    && existing.count() < existing.definition().max_stack_size =>
            {
                *target =
                    ItemStack::with_data(existing.item(), existing.count() + 1, existing.data())
                        .ok();
                *carried = ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
            }
            _ => {}
        }
    } else {
        if let (Some(stack), Some(existing)) = (*carried, target.as_mut()) {
            if existing.item() == stack.item() && existing.data() == stack.data() {
                *carried = existing.merge(stack);
                return;
            }
        }
        std::mem::swap(target, carried);
    }
}

fn take_workbench_result(
    session: &mut WorkbenchUiSession,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
) -> bool {
    let grid = session.grid.clone();
    let Some(output) = beta_recipe_book().find(&grid) else {
        return false;
    };
    let mut simulated_grid = grid.clone();
    let Some(remainders) = beta_recipe_book().consume_one(&mut simulated_grid) else {
        return false;
    };
    let mut simulated_inventory = inventory.clone();
    let mut simulated_hotbar = hotbar.clone();
    for remainder in remainders {
        if simulated_inventory
            .insert(&mut simulated_hotbar, remainder)
            .is_some()
        {
            return false;
        }
    }
    match simulated_inventory.carried {
        None => simulated_inventory.carried = Some(output),
        Some(mut carried) if carried.item() == output.item() && carried.data() == output.data() => {
            if carried.merge(output).is_some() {
                return false;
            }
            simulated_inventory.carried = Some(carried);
        }
        Some(_) => return false,
    }
    session.grid = simulated_grid;
    *inventory = simulated_inventory;
    *hotbar = simulated_hotbar;
    true
}

fn handle_slots(
    screen: Res<InventoryScreen>,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    slots: Query<(&RelativeCursorPosition, &Slot)>,
    mut player: Query<(&mut Hotbar, &mut Inventory), With<Player>>,
    mut workbench: ResMut<WorkbenchUiSession>,
    mut chunks: ResMut<WorldChunks>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut drag: ResMut<SlotDrag>,
    mut last_click: ResMut<LastInventoryClick>,
) {
    if !screen.open {
        *drag = SlotDrag::default();
        *last_click = LastInventoryClick::default();
        return;
    }
    let Ok((mut hotbar, mut inventory)) = player.single_mut() else {
        return;
    };
    if screen.chest {
        handle_chest_slots(
            &screen,
            &mouse,
            &keys,
            &slots,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
            &mut chunks,
            &mut persistence,
            &mut drag,
            &mut last_click,
            time.elapsed_secs_f64(),
        );
        return;
    }
    let hovered = slots
        .iter()
        .find_map(|(cursor, slot)| cursor.cursor_over().then_some(*slot));
    if keys.just_pressed(KeyCode::KeyS) && drag.button.is_none() {
        if matches!(hovered, Some(Slot::Main(_))) {
            sort_main_inventory(&mut inventory);
        }
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if screen.furnace {
        let Some(position) = screen.furnace_position else {
            return;
        };
        if let Some(Slot::Furnace(index @ 0..=1)) = hovered {
            for (hotbar_index, key) in HOTBAR_KEYS.iter().enumerate() {
                if !keys.just_pressed(*key) {
                    continue;
                }
                if let Some(furnace) = chunks.furnace_at_mut(position.0, position.1, position.2) {
                    std::mem::swap(&mut furnace.slots[index], &mut hotbar.slots[hotbar_index]);
                }
            }
        } else if let Some(slot) = hovered.filter(|slot| !matches!(slot, Slot::Furnace(_))) {
            for (index, key) in HOTBAR_KEYS.iter().enumerate() {
                if keys.just_pressed(*key) {
                    let _ = hotbar_key_swap(
                        &mut inventory,
                        &mut hotbar,
                        Some(&mut workbench.grid),
                        false,
                        to_slot_id(slot),
                        index,
                    );
                }
            }
        }
        if drag.button.is_none()
            && shift
            && mouse.just_pressed(MouseButton::Left)
            && let Some(slot) = hovered
        {
            drag.button = Some(MouseButton::Left);
            drag.origin = Some(slot);
            drag.slots.clear();
            drag.quick_move = true;
            drag.quick_move_visited.clear();
            last_click.at = None;
        }
        if drag.quick_move {
            if shift && let Some(slot) = hovered {
                let moved = if let Some(furnace) =
                    chunks.furnace_at_mut(position.0, position.1, position.2)
                {
                    quick_move_drag_slot(
                        &mut drag.quick_move_visited,
                        &mut inventory,
                        &mut hotbar,
                        None,
                        false,
                        None,
                        Some(&mut furnace.slots),
                        to_slot_id(slot),
                    )
                } else {
                    false
                };
                if moved && let Some(persistence) = persistence.as_deref_mut() {
                    persistence.mark_dirty(ChunkPos::from_block(position.0, position.2));
                }
            }
            if mouse.pressed(MouseButton::Left) {
                return;
            }
            *drag = SlotDrag::default();
            return;
        }
        if let Some(slot) = hovered
            && (mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right))
        {
            let right =
                mouse.just_pressed(MouseButton::Right) && !mouse.just_pressed(MouseButton::Left);
            if !right
                && !shift
                && is_double_click(&mut last_click, &screen, slot, time.elapsed_secs_f64())
            {
                collect_open_inventory(
                    &screen,
                    slot,
                    &mut chunks,
                    &mut hotbar,
                    &mut inventory,
                    &mut workbench,
                );
                if matches!(slot, Slot::Furnace(_))
                    && let Some(persistence) = persistence.as_deref_mut()
                {
                    persistence.mark_dirty(ChunkPos::from_block(position.0, position.2));
                }
                *drag = SlotDrag::default();
                return;
            }
            if right || shift {
                last_click.at = None;
            }
            if shift {
                shift_click_furnace(slot, position, &mut chunks, &mut hotbar, &mut inventory);
            } else {
                apply_furnace_click(
                    slot,
                    right,
                    position,
                    &mut chunks,
                    &mut hotbar,
                    &mut inventory,
                    &mut workbench,
                );
            }
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(ChunkPos::from_block(position.0, position.2));
            }
        } else if hovered.is_some() && HOTBAR_KEYS.iter().any(|key| keys.just_pressed(*key)) {
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(ChunkPos::from_block(position.0, position.2));
            }
        }
        *drag = SlotDrag::default();
        return;
    }
    if let Some(slot) = hovered {
        for (index, key) in HOTBAR_KEYS.iter().enumerate() {
            if keys.just_pressed(*key) {
                *drag = SlotDrag::default();
                let _ = hotbar_key_swap(
                    &mut inventory,
                    &mut hotbar,
                    Some(&mut workbench.grid),
                    screen.workbench,
                    to_slot_id(slot),
                    index,
                );
            }
        }
    }
    if drag.button.is_none() {
        let left = mouse.just_pressed(MouseButton::Left);
        let right = mouse.just_pressed(MouseButton::Right);
        if !(left || right) {
            return;
        }
        let Some(slot) = hovered else {
            return;
        };
        let right = right && !left;
        if !right
            && !shift
            && is_double_click(&mut last_click, &screen, slot, time.elapsed_secs_f64())
        {
            collect_open_inventory(
                &screen,
                slot,
                &mut chunks,
                &mut hotbar,
                &mut inventory,
                &mut workbench,
            );
            *drag = SlotDrag::default();
            return;
        }
        if right || shift {
            last_click.at = None;
        }
        if shift && !right {
            drag.button = Some(MouseButton::Left);
            drag.origin = Some(slot);
            drag.slots.clear();
            drag.quick_move = true;
            drag.quick_move_visited.clear();
        } else if shift {
            let _ = shift_click_slot(
                &mut inventory,
                &mut hotbar,
                Some(&mut workbench.grid),
                screen.workbench,
                to_slot_id(slot),
            );
            return;
        }
        if !drag.quick_move && inventory.carried.is_none() {
            apply_click(
                slot,
                right,
                screen.workbench,
                &mut hotbar,
                &mut inventory,
                &mut workbench,
            );
            return;
        }
        if !drag.quick_move {
            drag.button = Some(if right {
                MouseButton::Right
            } else {
                MouseButton::Left
            });
            drag.origin = Some(slot);
            drag.slots.clear();
        }
    }
    let Some(button) = drag.button else {
        return;
    };
    if drag.quick_move {
        if shift && let Some(slot) = hovered {
            let _ = quick_move_drag_slot(
                &mut drag.quick_move_visited,
                &mut inventory,
                &mut hotbar,
                Some(&mut workbench.grid),
                screen.workbench,
                None,
                None,
                to_slot_id(slot),
            );
        }
        if mouse.pressed(button) {
            return;
        }
        *drag = SlotDrag::default();
        return;
    }
    if let Some(slot) = hovered {
        remember_drag_slot(&mut drag, slot, &inventory, &hotbar, &workbench.grid);
    }
    if mouse.pressed(button) {
        return;
    }
    let mode = if button == MouseButton::Right {
        DragPlace::OneEach
    } else {
        DragPlace::Split
    };
    let painted = drag.slots.clone();
    let origin = drag.origin;
    *drag = SlotDrag::default();
    if painted.len() >= 2 {
        let ids: Vec<SlotId> = painted.into_iter().map(to_slot_id).collect();
        let _ = drag_place(
            &mut inventory,
            &mut hotbar,
            Some(&mut workbench.grid),
            &ids,
            mode,
        );
        return;
    }
    let click = hovered.or(if painted.len() == 1 {
        painted.first().copied()
    } else {
        origin
    });
    if let Some(slot) = click {
        apply_click(
            slot,
            mode == DragPlace::OneEach,
            screen.workbench,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_chest_slots(
    screen: &InventoryScreen,
    mouse: &ButtonInput<MouseButton>,
    keys: &ButtonInput<KeyCode>,
    slots: &Query<(&RelativeCursorPosition, &Slot)>,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut WorkbenchUiSession,
    chunks: &mut WorldChunks,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    drag: &mut SlotDrag,
    last_click: &mut LastInventoryClick,
    now: f64,
) {
    let Some(group) = screen.chest_group else {
        return;
    };
    let hovered = slots
        .iter()
        .find_map(|(cursor, slot)| cursor.cursor_over().then_some(*slot));
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if let Some(slot) = hovered {
        for (index, key) in HOTBAR_KEYS.iter().enumerate() {
            if keys.just_pressed(*key) {
                *drag = SlotDrag::default();
                let mut chest_slots = read_chest_group_slots(chunks, group);
                let _ = hotbar_key_swap_chest(
                    inventory,
                    hotbar,
                    &mut chest_slots,
                    to_slot_id(slot),
                    index,
                );
                write_chest_group_slots(chunks, group, &chest_slots);
                mark_chest_dirty(persistence, group);
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyS) && drag.button.is_none() {
        match hovered {
            Some(Slot::Chest(_)) => {
                let mut chest_slots = read_chest_group_slots(chunks, group);
                sort_container_slots(&mut chest_slots);
                write_chest_group_slots(chunks, group, &chest_slots);
                mark_chest_dirty(persistence, group);
            }
            Some(Slot::Main(_)) => sort_main_inventory(inventory),
            _ => {}
        }
        return;
    }
    if drag.button.is_none() {
        let left = mouse.just_pressed(MouseButton::Left);
        let right = mouse.just_pressed(MouseButton::Right);
        if !(left || right) {
            return;
        }
        let Some(slot) = hovered else {
            return;
        };
        let right = right && !left;
        if !right && !shift && is_double_click(last_click, screen, slot, now) {
            if matches!(slot, Slot::Chest(_)) {
                let mut chest_slots = read_chest_group_slots(chunks, group);
                collect_matching_stacks(&mut inventory.carried, &mut chest_slots);
                write_chest_group_slots(chunks, group, &chest_slots);
                mark_chest_dirty(persistence, group);
            } else {
                collect_storage_stacks(inventory, hotbar);
            }
            *drag = SlotDrag::default();
            return;
        }
        if right || shift {
            last_click.at = None;
        }
        if shift && !right {
            drag.button = Some(MouseButton::Left);
            drag.origin = Some(slot);
            drag.slots.clear();
            drag.quick_move = true;
            drag.quick_move_visited.clear();
        } else if shift {
            let mut chest_slots = read_chest_group_slots(chunks, group);
            let _ = shift_click_chest_slot(inventory, hotbar, &mut chest_slots, to_slot_id(slot));
            write_chest_group_slots(chunks, group, &chest_slots);
            mark_chest_dirty(persistence, group);
            return;
        }
        if !drag.quick_move && inventory.carried.is_none() {
            apply_chest_click(slot, right, group, chunks, hotbar, inventory, workbench);
            mark_chest_dirty(persistence, group);
            return;
        }
        if !drag.quick_move {
            drag.button = Some(if right {
                MouseButton::Right
            } else {
                MouseButton::Left
            });
            drag.origin = Some(slot);
            drag.slots.clear();
        }
    }
    let Some(button) = drag.button else {
        return;
    };
    if drag.quick_move {
        if shift && let Some(slot) = hovered {
            let mut chest_slots = read_chest_group_slots(chunks, group);
            let moved = quick_move_drag_slot(
                &mut drag.quick_move_visited,
                inventory,
                hotbar,
                Some(&mut workbench.grid),
                screen.workbench,
                Some(&mut chest_slots),
                None,
                to_slot_id(slot),
            );
            if moved {
                write_chest_group_slots(chunks, group, &chest_slots);
                mark_chest_dirty(persistence, group);
            }
        }
        if mouse.pressed(button) {
            return;
        }
        *drag = SlotDrag::default();
        return;
    }
    if let Some(slot) = hovered {
        let chest_slots = read_chest_group_slots(chunks, group);
        remember_chest_drag_slot(drag, slot, inventory, hotbar, &chest_slots);
    }
    if mouse.pressed(button) {
        return;
    }
    let mode = if button == MouseButton::Right {
        DragPlace::OneEach
    } else {
        DragPlace::Split
    };
    let painted = drag.slots.clone();
    let origin = drag.origin;
    *drag = SlotDrag::default();
    if painted.len() >= 2 {
        let ids: Vec<SlotId> = painted.into_iter().map(to_slot_id).collect();
        let mut chest_slots = read_chest_group_slots(chunks, group);
        let _ = chest_drag_place(inventory, hotbar, &mut chest_slots, &ids, mode);
        write_chest_group_slots(chunks, group, &chest_slots);
    } else if let Some(slot) = hovered.or(if painted.len() == 1 {
        painted.first().copied()
    } else {
        origin
    }) {
        apply_chest_click(
            slot,
            mode == DragPlace::OneEach,
            group,
            chunks,
            hotbar,
            inventory,
            workbench,
        );
    }
    mark_chest_dirty(persistence, group);
}

fn remember_chest_drag_slot(
    drag: &mut SlotDrag,
    slot: Slot,
    inventory: &Inventory,
    hotbar: &Hotbar,
    chest: &[Option<ItemStack>],
) {
    let Some(carried) = inventory.carried else {
        return;
    };
    if drag.slots.contains(&slot) || (carried.count() as usize) <= drag.slots.len() {
        return;
    }
    if chest_slot_accepts_drag(inventory, hotbar, chest, to_slot_id(slot), carried) {
        drag.slots.push(slot);
    }
}

fn apply_chest_click(
    slot: Slot,
    right: bool,
    group: ChestGroup,
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut WorkbenchUiSession,
) {
    if let Slot::Chest(index) = slot {
        let mut chest_slots = read_chest_group_slots(chunks, group);
        if let Some(stack) = chest_slots.get_mut(index) {
            click_slot(stack, &mut inventory.carried, right);
        }
        write_chest_group_slots(chunks, group, &chest_slots);
    } else {
        apply_click(slot, right, false, hotbar, inventory, workbench);
    }
}

fn collect_player_stacks(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    workbench: &mut WorkbenchUiSession,
    workbench_open: bool,
) {
    collect_storage_stacks(inventory, hotbar);
    let Inventory {
        crafting,
        armor,
        carried,
        ..
    } = inventory;
    if workbench_open {
        collect_matching_stacks(carried, workbench.grid.slots_mut());
    } else {
        collect_matching_stacks(carried, crafting);
    }
    collect_matching_stacks(carried, armor);
}

fn collect_storage_stacks(inventory: &mut Inventory, hotbar: &mut Hotbar) {
    let Inventory { main, carried, .. } = inventory;
    collect_matching_stacks(carried, &mut hotbar.slots);
    collect_matching_stacks(carried, main);
}

fn collect_open_inventory(
    screen: &InventoryScreen,
    clicked: Slot,
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut WorkbenchUiSession,
) {
    if screen.furnace {
        if matches!(clicked, Slot::Furnace(_)) {
            if let Some(position) = screen.furnace_position
                && let Some(furnace) = chunks.furnace_at_mut(position.0, position.1, position.2)
            {
                collect_matching_stacks(&mut inventory.carried, &mut furnace.slots);
            }
        } else {
            collect_storage_stacks(inventory, hotbar);
        }
    } else {
        collect_player_stacks(inventory, hotbar, workbench, screen.workbench);
    }
}

fn read_chest_group_slots(
    chunks: &WorldChunks,
    group: ChestGroup,
) -> [Option<ItemStack>; DOUBLE_CHEST_SLOTS] {
    let mut slots = [None; DOUBLE_CHEST_SLOTS];
    if let Some(chest) = chunks.chest_at(group.first.0, group.first.1, group.first.2) {
        slots[..CHEST_HALF_SLOTS].copy_from_slice(&chest.slots);
    }
    if let Some((x, y, z)) = group.second
        && let Some(chest) = chunks.chest_at(x, y, z)
    {
        slots[CHEST_HALF_SLOTS..].copy_from_slice(&chest.slots);
    }
    slots
}

fn write_chest_group_slots(
    chunks: &mut WorldChunks,
    group: ChestGroup,
    slots: &[Option<ItemStack>; DOUBLE_CHEST_SLOTS],
) {
    if let Some(chest) = chunks.chest_at_mut(group.first.0, group.first.1, group.first.2) {
        chest.slots.copy_from_slice(&slots[..CHEST_HALF_SLOTS]);
    }
    if let Some((x, y, z)) = group.second
        && let Some(chest) = chunks.chest_at_mut(x, y, z)
    {
        chest.slots.copy_from_slice(&slots[CHEST_HALF_SLOTS..]);
    }
}

fn mark_chest_dirty(persistence: &mut Option<ResMut<WorldPersistence>>, group: ChestGroup) {
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.mark_dirty(ChunkPos::from_block(group.first.0, group.first.2));
        if let Some((x, _, z)) = group.second {
            persistence.mark_dirty(ChunkPos::from_block(x, z));
        }
    }
}

fn remember_drag_slot(
    drag: &mut SlotDrag,
    slot: Slot,
    inventory: &Inventory,
    hotbar: &Hotbar,
    workbench: &CraftingGrid,
) {
    let Some(carried) = inventory.carried else {
        return;
    };
    if drag.slots.contains(&slot) || (carried.count() as usize) <= drag.slots.len() {
        return;
    }
    if slot_accepts_drag(
        inventory,
        hotbar,
        Some(workbench),
        to_slot_id(slot),
        carried,
    ) {
        drag.slots.push(slot);
    }
}

fn to_slot_id(slot: Slot) -> SlotId {
    match slot {
        Slot::Hotbar(index) => SlotId::Hotbar(index),
        Slot::Main(index) => SlotId::Main(index),
        Slot::Craft(index) => SlotId::Craft(index),
        Slot::CraftResult => SlotId::CraftResult,
        Slot::Workbench(index) => SlotId::Workbench(index),
        Slot::Chest(index) => SlotId::Chest(index),
        Slot::Furnace(index) => SlotId::Furnace(index),
        Slot::Armor(index) => SlotId::Armor(index),
    }
}

fn apply_furnace_click(
    slot: Slot,
    right: bool,
    position: (i32, i32, i32),
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut WorkbenchUiSession,
) {
    let Some(furnace) = chunks.furnace_at_mut(position.0, position.1, position.2) else {
        return;
    };
    match slot {
        Slot::Furnace(index) if index < 2 => {
            click_slot(&mut furnace.slots[index], &mut inventory.carried, right);
        }
        Slot::Furnace(2) => {
            if inventory.carried.is_some() {
                return;
            }
            let Some(stack) = furnace.slots[2] else {
                return;
            };
            let count = if right {
                stack.count().div_ceil(2)
            } else {
                stack.count()
            };
            inventory.carried = ItemStack::with_data(stack.item(), count, stack.data()).ok();
            furnace.slots[2] =
                ItemStack::with_data(stack.item(), stack.count() - count, stack.data()).ok();
        }
        Slot::Furnace(_) => {}
        _ => apply_click(slot, right, false, hotbar, inventory, workbench),
    }
}

fn shift_click_furnace(
    slot: Slot,
    position: (i32, i32, i32),
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
) -> bool {
    let Some(furnace) = chunks.furnace_at_mut(position.0, position.1, position.2) else {
        return false;
    };
    shift_click_furnace_slot(inventory, hotbar, &mut furnace.slots, to_slot_id(slot))
}

fn apply_click(
    slot: Slot,
    right: bool,
    workbench_open: bool,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut WorkbenchUiSession,
) {
    match slot {
        Slot::Hotbar(i) => click_slot(&mut hotbar.slots[i], &mut inventory.carried, right),
        Slot::Main(i) => {
            let Inventory { main, carried, .. } = &mut *inventory;
            click_slot(&mut main[i], carried, right);
        }
        Slot::Craft(i) => {
            let Inventory {
                crafting, carried, ..
            } = &mut *inventory;
            click_slot(&mut crafting[i], carried, right);
        }
        Slot::CraftResult if !right => {
            if workbench_open {
                let _ = take_workbench_result(workbench, hotbar, inventory);
            } else if inventory.carried.is_none()
                || inventory.carried.is_some_and(|carried| {
                    inventory.crafting_result().is_some_and(|result| {
                        carried.item() == result.item()
                            && carried.data() == result.data()
                            && carried.count() + result.count()
                                <= carried.definition().max_stack_size
                    })
                })
            {
                let _ = inventory.take_crafting_result(hotbar);
            }
        }
        Slot::CraftResult => {}
        Slot::Workbench(i) => {
            let x = i % 3;
            let y = i / 3;
            let mut value = workbench.grid.get(x, y);
            click_slot(&mut value, &mut inventory.carried, right);
            workbench.grid.set(x, y, value);
        }
        Slot::Chest(_) => {}
        Slot::Armor(i) => {
            let Inventory { armor, carried, .. } = &mut *inventory;
            click_slot(&mut armor[i], carried, right);
        }
        Slot::Furnace(_) => {}
    }
}

fn highlight_slots(
    screen: Res<InventoryScreen>,
    drag: Res<SlotDrag>,
    slots: Query<(&RelativeCursorPosition, &Slot, &Children)>,
    mut highlights: Query<&mut Visibility, With<SlotHighlight>>,
) {
    if !screen.open {
        return;
    }
    for (cursor, slot, children) in &slots {
        let show = cursor.cursor_over() || drag.slots.contains(slot);
        let visibility = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for child in children {
            if let Ok(mut highlight) = highlights.get_mut(*child) {
                highlight.set_if_neq(visibility);
            }
        }
    }
}

fn stack_text(stack: Option<ItemStack>) -> String {
    stack
        .map(|stack| {
            let label = stack.item().to_string();
            if stack.count() > 1 {
                format!("{label}\n{}", stack.count())
            } else {
                label
            }
        })
        .unwrap_or_default()
}

fn refresh(
    screen: Res<InventoryScreen>,
    player: Query<(&Hotbar, &Inventory), With<Player>>,
    workbench: Res<WorkbenchUiSession>,
    chunks: Res<WorldChunks>,
    drag: Res<SlotDrag>,
    mut labels: Query<
        (
            &SlotLabel,
            &mut Text,
            &mut Node,
            &mut TextLayout,
            &mut TextFont,
            &mut LineHeight,
            &mut TextShadow,
        ),
        (
            Without<SlotDurability>,
            Without<CarriedLabel>,
            Without<CarriedIcon>,
            Without<FurnaceProgress>,
        ),
    >,
    mut bars: Query<
        (
            &SlotDurability,
            &mut Node,
            &mut Visibility,
            &mut BackgroundColor,
        ),
        (
            Without<SlotLabel>,
            Without<CarriedLabel>,
            Without<CarriedIcon>,
            Without<FurnaceProgress>,
        ),
    >,
    mut furnace_progress: Query<
        (&FurnaceProgress, &mut ImageNode, &mut Node, &mut Visibility),
        (
            Without<SlotLabel>,
            Without<SlotDurability>,
            Without<CarriedLabel>,
            Without<CarriedIcon>,
            Without<SlotIcon>,
        ),
    >,
    mut icons: Query<
        (&SlotIcon, &mut ImageNode, &mut Visibility),
        (
            Without<CarriedIcon>,
            Without<SlotDurability>,
            Without<FurnaceProgress>,
        ),
    >,
    mut carried: Query<
        (
            &mut Text,
            &mut Node,
            &mut TextLayout,
            &mut TextFont,
            &mut LineHeight,
            &mut TextShadow,
        ),
        (
            With<CarriedLabel>,
            Without<SlotLabel>,
            Without<CarriedIcon>,
            Without<SlotDurability>,
        ),
    >,
    mut carried_icon: Query<
        (&mut ImageNode, &mut Visibility, &mut Node),
        (
            With<CarriedIcon>,
            Without<SlotIcon>,
            Without<CarriedLabel>,
            Without<SlotDurability>,
        ),
    >,
    windows: Query<&Window, With<PrimaryWindow>>,
    block_icons: Res<BlockIcons>,
    font: Res<UiFont>,
) {
    if !screen.open {
        return;
    }
    let Ok((hotbar, inventory)) = player.single() else {
        return;
    };
    let preview_slots: Vec<_> = drag.slots.iter().copied().map(to_slot_id).collect();
    let preview = drag.button.map_or_else(Vec::new, |button| {
        let mode = if button == MouseButton::Right {
            DragPlace::OneEach
        } else {
            DragPlace::Split
        };
        if screen.chest {
            screen
                .chest_group
                .map(|group| read_chest_group_slots(&chunks, group))
                .map_or_else(Vec::new, |chest| {
                    preview_chest_drag_place(inventory, hotbar, &chest, &preview_slots, mode)
                })
        } else {
            preview_drag_place(
                inventory,
                hotbar,
                Some(&workbench.grid),
                &preview_slots,
                mode,
            )
        }
    });
    for (label, mut text, mut node, mut layout, mut text_font, mut line_height, mut shadow) in
        &mut labels
    {
        let stack = displayed_stack(
            label.0,
            hotbar,
            inventory,
            &workbench,
            screen.workbench,
            screen.furnace_position,
            screen.chest_group,
            &chunks,
            &preview,
        );
        let has_icon = stack
            .and_then(|stack| block_icons.rect_for_stack(stack))
            .is_some();
        let label_text = if has_icon {
            count_label(stack)
        } else {
            stack_text(stack)
        };
        place_stack_label(
            &mut text,
            &mut node,
            &mut layout,
            &mut text_font,
            &mut line_height,
            &mut shadow,
            &font.minecraft,
            0.0,
            0.0,
            &label_text,
            has_icon,
        );
    }
    for (bar, mut node, mut visibility, mut color) in &mut bars {
        let stack = displayed_stack(
            bar.0,
            hotbar,
            inventory,
            &workbench,
            screen.workbench,
            screen.furnace_position,
            screen.chest_group,
            &chunks,
            &preview,
        );
        if let Some((width, red, green)) = stack.and_then(durability_bar) {
            *visibility = Visibility::Inherited;
            let (_, _, track_width, _) = durability_track(0.0, 0.0, bar.1);
            if bar.1 {
                node.width = px(width);
                *color = BackgroundColor(Color::srgb_u8(red, green, 0));
            } else {
                node.width = px(track_width);
                *color = BackgroundColor(Color::BLACK);
            }
        } else {
            *visibility = Visibility::Hidden;
        }
    }
    for (icon, mut image, mut visibility) in &mut icons {
        let stack = displayed_stack(
            icon.0,
            hotbar,
            inventory,
            &workbench,
            screen.workbench,
            screen.furnace_position,
            screen.chest_group,
            &chunks,
            &preview,
        );
        if let Some(rect) = stack.and_then(|stack| block_icons.rect_for_stack(stack)) {
            image.rect = Some(rect);
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
    let furnace = screen
        .furnace_position
        .and_then(|(x, y, z)| chunks.furnace_at(x, y, z));
    for (indicator, mut image, mut node, mut visibility) in &mut furnace_progress {
        let progress = furnace.map_or(0.0, |furnace| {
            if indicator.0 {
                f32::from(furnace.cook_ticks) / f32::from(crate::world::furnace::SMELT_TICKS)
            } else if furnace.fuel_ticks > 0 {
                f32::from(furnace.burn_ticks) / f32::from(furnace.fuel_ticks)
            } else {
                0.0
            }
        });
        if progress <= 0.0 {
            *visibility = Visibility::Hidden;
        } else {
            *visibility = Visibility::Inherited;
            if indicator.0 {
                let width = (24.0 * progress).ceil().clamp(1.0, 24.0);
                image.rect = Some(Rect::new(176.0, 14.0, 176.0 + width, 30.0));
                node.width = px(width * SCALE);
                node.height = px(16.0 * SCALE);
                node.left = px(79.0 * SCALE);
                node.top = px(34.0 * SCALE);
            } else {
                let height = (14.0 * progress).ceil().clamp(1.0, 14.0);
                image.rect = Some(Rect::new(176.0, 14.0 - height, 190.0, 14.0));
                node.height = px(height * SCALE);
                node.top = px((50.0 - height) * SCALE);
                node.left = px(56.0 * SCALE);
                node.width = px(14.0 * SCALE);
            }
        }
    }
    let cursor_icon = windows.single().ok().and_then(|window| {
        window.cursor_position().map(|pos| {
            // GuiContainer draws the carried stack at the cursor minus half an icon.
            (
                pos.x - (window.width() - 176.0 * SCALE) / 2.0 - 8.0 * SCALE,
                pos.y
                    - (window.height()
                        - (if screen.chest {
                            114.0
                                + screen.chest_group.map_or(3, |group| group.slot_count() / 9)
                                    as f32
                                    * 18.0
                        } else {
                            166.0
                        }) * SCALE)
                        / 2.0
                    - 8.0 * SCALE,
            )
        })
    });
    if let Ok((mut image, mut visibility, mut node)) = carried_icon.single_mut() {
        if let Some(rect) = inventory
            .carried
            .and_then(|stack| block_icons.rect_for_stack(stack))
        {
            image.rect = Some(rect);
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
        }
        if let Some((left, top)) = cursor_icon {
            node.left = px(left);
            node.top = px(top);
        }
    }
    if let Ok((mut text, mut node, mut layout, mut text_font, mut line_height, mut shadow)) =
        carried.single_mut()
    {
        let has_icon = block_icons.ready()
            && inventory
                .carried
                .and_then(|stack| block_icons.rect_for_stack(stack))
                .is_some();
        let label_text = if has_icon {
            count_label(inventory.carried)
        } else {
            stack_text(inventory.carried)
        };
        if let Some((left, top)) = cursor_icon {
            place_stack_label(
                &mut text,
                &mut node,
                &mut layout,
                &mut text_font,
                &mut line_height,
                &mut shadow,
                &font.minecraft,
                left,
                top,
                &label_text,
                has_icon,
            );
        } else {
            **text = label_text;
        }
    }
}

fn displayed_stack(
    slot: Slot,
    hotbar: &Hotbar,
    inventory: &Inventory,
    workbench: &WorkbenchUiSession,
    workbench_open: bool,
    furnace_position: Option<(i32, i32, i32)>,
    chest_group: Option<ChestGroup>,
    chunks: &WorldChunks,
    preview: &[(SlotId, ItemStack)],
) -> Option<ItemStack> {
    preview
        .iter()
        .find_map(|(preview_slot, stack)| (*preview_slot == to_slot_id(slot)).then_some(*stack))
        .or_else(|| {
            slot_stack(
                slot,
                hotbar,
                inventory,
                workbench,
                workbench_open,
                furnace_position,
                chest_group,
                chunks,
            )
        })
}

fn slot_stack(
    slot: Slot,
    hotbar: &Hotbar,
    inventory: &Inventory,
    workbench: &WorkbenchUiSession,
    workbench_open: bool,
    furnace_position: Option<(i32, i32, i32)>,
    chest_group: Option<ChestGroup>,
    chunks: &WorldChunks,
) -> Option<ItemStack> {
    match slot {
        Slot::Hotbar(i) => hotbar.slots[i],
        Slot::Main(i) => inventory.main[i],
        Slot::Craft(i) => inventory.crafting[i],
        Slot::CraftResult if workbench_open => beta_recipe_book().find(&workbench.grid),
        Slot::CraftResult => inventory.crafting_result(),
        Slot::Workbench(i) => workbench.grid.get(i % 3, i / 3),
        Slot::Chest(i) => chest_group
            .map(|group| read_chest_group_slots(chunks, group))
            .and_then(|slots| slots.get(i).copied().flatten()),
        Slot::Furnace(i) => furnace_position
            .and_then(|(x, y, z)| chunks.furnace_at(x, y, z))
            .and_then(|furnace| furnace.slots.get(i).copied().flatten()),
        Slot::Armor(i) => inventory.armor[i],
    }
}

pub(super) fn durability_bar(stack: ItemStack) -> Option<(f32, u8, u8)> {
    let ItemData::Durability(max) = stack.definition().data else {
        return None;
    };
    if stack.data() == 0 || max == 0 {
        return None;
    }
    let fraction = stack.data() as f32 / max as f32;
    let width = (13.0 - fraction * 13.0).round().clamp(0.0, 13.0) * SCALE;
    let green = (255.0 - fraction * 255.0).round().clamp(0.0, 255.0) as u8;
    Some((width, 255 - green, green))
}
