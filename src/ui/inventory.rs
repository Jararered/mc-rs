use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::text::Justify;
use bevy::text::LineHeight;
use bevy::text::TextLayout;
use bevy::ui::RelativeCursorPosition;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use super::block_icons::BlockIcons;
use super::stack_overlay::GUI_SCALE;
use super::stack_overlay::UiFont;
use super::stack_overlay::count_frame;
use super::stack_overlay::count_label;
use super::stack_overlay::count_line_height;
use super::stack_overlay::count_shadow;
use super::stack_overlay::count_text_font;
use super::stack_overlay::durability_track;
use super::stack_overlay::icon_size;
use super::stack_overlay::place_stack_label;
use crate::app::state::AppScreen;
use crate::crafting::CraftingGrid;
use crate::crafting::beta_recipe_book;
use crate::entity::dropped_items::ItemRng;
use crate::entity::dropped_items::spawn_thrown_item;
use crate::inventory::DragPlace;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::SlotId;
use crate::inventory::drag_place;
use crate::inventory::hotbar_key_swap;
use crate::inventory::shift_click_slot;
use crate::inventory::slot_accepts_drag;
use crate::item::ItemData;
use crate::item::ItemStack;
use crate::player::Player;
use crate::world::block::block::BlockId;
use crate::world::chunk::WorldChunks;

const SCALE: f32 = GUI_SCALE;
const SLOT_SIZE: f32 = 16.0;
const SLOT_STEP: f32 = 18.0;

pub struct InventoryGuiPlugin;

impl Plugin for InventoryGuiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryScreen>()
            .init_resource::<WorkbenchUiSession>()
            .init_resource::<SlotDrag>()
            .add_systems(PreStartup, (load_texture, super::block_icons::setup))
            .add_systems(Update, super::block_icons::build.before(refresh))
            .add_systems(
                Update,
                (
                    validate_workbench,
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
    if chunks.block_at(x, y, z) == Some(BlockId::CraftingTable)
        && dx * dx + dy * dy + dz * dz <= 64.0
    {
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

#[derive(Resource, Default)]
pub(crate) struct InventoryScreen {
    pub open: bool,
    pub workbench: bool,
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
}

fn load_texture(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(InventoryTexture {
        background: assets.load("gui/inventory.png"),
        crafting: assets.load("gui/crafting.png"),
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
        let background = if screen.workbench {
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
        let background = if screen.workbench {
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
) {
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
                ImageNode::new(texture.clone())
                    .with_rect(bevy::math::Rect::new(0.0, 0.0, 176.0, 166.0)),
                Node {
                    width: px(176.0 * SCALE),
                    height: px(166.0 * SCALE),
                    ..default()
                },
            ))
            .with_children(|panel| {
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
                if workbench {
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
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    slots: Query<(&RelativeCursorPosition, &Slot)>,
    mut player: Query<(&mut Hotbar, &mut Inventory), With<Player>>,
    mut workbench: ResMut<WorkbenchUiSession>,
    mut drag: ResMut<SlotDrag>,
) {
    if !screen.open {
        *drag = SlotDrag::default();
        return;
    }
    let Ok((mut hotbar, mut inventory)) = player.single_mut() else {
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
        if shift {
            let _ = shift_click_slot(
                &mut inventory,
                &mut hotbar,
                Some(&mut workbench.grid),
                screen.workbench,
                to_slot_id(slot),
            );
            return;
        }
        if inventory.carried.is_none() {
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
        drag.button = Some(if right {
            MouseButton::Right
        } else {
            MouseButton::Left
        });
        drag.origin = Some(slot);
        drag.slots.clear();
    }
    let Some(button) = drag.button else {
        return;
    };
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
        Slot::Armor(index) => SlotId::Armor(index),
    }
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
        Slot::Armor(i) => {
            let Inventory { armor, carried, .. } = &mut *inventory;
            click_slot(&mut armor[i], carried, right);
        }
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
            let name = stack.definition().name.replace('_', " ");
            if stack.count() > 1 {
                format!("{name}\n{}", stack.count())
            } else {
                name
            }
        })
        .unwrap_or_default()
}

fn refresh(
    screen: Res<InventoryScreen>,
    player: Query<(&Hotbar, &Inventory), With<Player>>,
    workbench: Res<WorkbenchUiSession>,
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
        ),
    >,
    mut icons: Query<
        (&SlotIcon, &mut ImageNode, &mut Visibility),
        (Without<CarriedIcon>, Without<SlotDurability>),
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
    for (label, mut text, mut node, mut layout, mut text_font, mut line_height, mut shadow) in
        &mut labels
    {
        let stack = slot_stack(label.0, hotbar, inventory, &workbench, screen.workbench);
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
        let stack = slot_stack(bar.0, hotbar, inventory, &workbench, screen.workbench);
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
        let stack = slot_stack(icon.0, hotbar, inventory, &workbench, screen.workbench);
        if let Some(rect) = stack.and_then(|stack| block_icons.rect_for_stack(stack)) {
            image.rect = Some(rect);
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
    let cursor_icon = windows.single().ok().and_then(|window| {
        window.cursor_position().map(|pos| {
            // GuiContainer draws the carried stack at the cursor minus half an icon.
            (
                pos.x - (window.width() - 176.0 * SCALE) / 2.0 - 8.0 * SCALE,
                pos.y - (window.height() - 166.0 * SCALE) / 2.0 - 8.0 * SCALE,
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

fn slot_stack(
    slot: Slot,
    hotbar: &Hotbar,
    inventory: &Inventory,
    workbench: &WorkbenchUiSession,
    workbench_open: bool,
) -> Option<ItemStack> {
    match slot {
        Slot::Hotbar(i) => hotbar.slots[i],
        Slot::Main(i) => inventory.main[i],
        Slot::Craft(i) => inventory.crafting[i],
        Slot::CraftResult if workbench_open => beta_recipe_book().find(&workbench.grid),
        Slot::CraftResult => inventory.crafting_result(),
        Slot::Workbench(i) => workbench.grid.get(i % 3, i / 3),
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
