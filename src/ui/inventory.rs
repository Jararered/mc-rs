use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use super::block_icons::BlockIcons;
use crate::app::state::AppScreen;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemData;
use crate::item::ItemStack;
use crate::player::Player;

const SCALE: f32 = 2.0;

pub struct InventoryGuiPlugin;

impl Plugin for InventoryGuiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryScreen>()
            .add_systems(PreStartup, (load_texture, super::block_icons::setup))
            .add_systems(Update, super::block_icons::build)
            .add_systems(
                Update,
                (toggle, handle_slots, refresh)
                    .chain()
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_systems(OnExit(AppScreen::Playing), close);
    }
}

#[derive(Resource, Default)]
pub(crate) struct InventoryScreen {
    pub open: bool,
}
#[derive(Resource)]
struct InventoryTexture {
    background: Handle<Image>,
}
#[derive(Component)]
struct InventoryRoot;
#[derive(Component, Clone, Copy)]
enum Slot {
    Hotbar(usize),
    Main(usize),
    Craft(usize),
    Armor(usize),
}
#[derive(Component)]
struct SlotLabel(Slot);
#[derive(Component)]
struct SlotIcon(Slot);
#[derive(Component)]
struct SlotDurability(Slot, bool);
#[derive(Component)]
struct CarriedLabel;
#[derive(Component)]
struct CarriedIcon;

fn load_texture(mut commands: Commands, assets: Res<AssetServer>) {
    let texture = assets
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::nearest()
        })
        .load("gui/inventory.png");
    commands.insert_resource(InventoryTexture {
        background: texture,
    });
}

fn toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut screen: ResMut<InventoryScreen>,
    texture: Res<InventoryTexture>,
    icons: Res<BlockIcons>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::KeyE) && !(screen.open && keys.just_pressed(KeyCode::Escape)) {
        return;
    }
    screen.open = !screen.open;
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
        spawn(&mut commands, &texture.background, &icons.image);
    } else {
        for root in &roots {
            commands.entity(root).despawn();
        }
    }
}

fn close(
    mut commands: Commands,
    mut screen: ResMut<InventoryScreen>,
    roots: Query<Entity, With<InventoryRoot>>,
) {
    screen.open = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
}

fn spawn(commands: &mut Commands, texture: &Handle<Image>, icons: &Handle<Image>) {
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
                        Slot::Main(index),
                        8.0 + (index % 9) as f32 * 18.0,
                        84.0 + (index / 9) as f32 * 18.0,
                    );
                }
                for index in 0..9 {
                    slot(
                        panel,
                        icons,
                        Slot::Hotbar(index),
                        8.0 + index as f32 * 18.0,
                        142.0,
                    );
                }
                for index in 0..4 {
                    slot(
                        panel,
                        icons,
                        Slot::Craft(index),
                        88.0 + (index % 2) as f32 * 18.0,
                        26.0 + (index / 2) as f32 * 18.0,
                    );
                }
                for index in 0..4 {
                    slot(
                        panel,
                        icons,
                        Slot::Armor(index),
                        8.0,
                        8.0 + index as f32 * 18.0,
                    );
                }
                panel.spawn((
                    CarriedIcon,
                    Pickable::IGNORE,
                    Visibility::Hidden,
                    ImageNode::new(icons.clone()),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(0),
                        width: px(32.0),
                        height: px(32.0),
                        ..default()
                    },
                ));
                panel.spawn((
                    CarriedLabel,
                    Pickable::IGNORE,
                    Text::new(""),
                    TextFont {
                        font_size: 15.0.into(),
                        ..default()
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(0),
                        ..default()
                    },
                ));
            });
        });
}

fn slot(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    icons: &Handle<Image>,
    id: Slot,
    x: f32,
    y: f32,
) {
    parent
        .spawn((
            Button,
            id,
            Node {
                position_type: PositionType::Absolute,
                left: px(x * SCALE),
                top: px(y * SCALE),
                width: px(18.0 * SCALE),
                height: px(18.0 * SCALE),
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
                    left: px(2.0),
                    top: px(2.0),
                    width: px(32.0),
                    height: px(32.0),
                    ..default()
                },
            ));
            button.spawn((
                SlotLabel(id),
                Pickable::IGNORE,
                Text::new(""),
                TextFont {
                    font_size: 15.0.into(),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextShadow::default(),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(2.0),
                    bottom: px(0.0),
                    ..default()
                },
            ));
            for (foreground, height, bottom) in [(false, 4.0, 4.0), (true, 2.0, 5.0)] {
                button.spawn((
                    SlotDurability(id, foreground),
                    Pickable::IGNORE,
                    Visibility::Hidden,
                    BackgroundColor(Color::BLACK),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(4.0),
                        bottom: px(bottom),
                        width: px(26.0),
                        height: px(height),
                        ..default()
                    },
                ));
            }
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

fn handle_slots(
    screen: Res<InventoryScreen>,
    mouse: Res<ButtonInput<MouseButton>>,
    buttons: Query<(&Interaction, &Slot), With<Button>>,
    mut player: Query<(&mut Hotbar, &mut Inventory), With<Player>>,
) {
    if !screen.open {
        return;
    }
    let Ok((mut hotbar, mut inventory)) = player.single_mut() else {
        return;
    };
    for (interaction, slot) in &buttons {
        if *interaction == Interaction::None
            || !(mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right))
        {
            continue;
        }
        let right = mouse.just_pressed(MouseButton::Right);
        match *slot {
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
            Slot::Armor(i) => {
                let Inventory { armor, carried, .. } = &mut *inventory;
                click_slot(&mut armor[i], carried, right);
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
    mut labels: Query<
        (&SlotLabel, &mut Text, &mut Node),
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
        (&mut Text, &mut Node),
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
) {
    if !screen.open {
        return;
    }
    let Ok((hotbar, inventory)) = player.single() else {
        return;
    };
    for (label, mut text, mut node) in &mut labels {
        let stack = match label.0 {
            Slot::Hotbar(i) => hotbar.slots[i],
            Slot::Main(i) => inventory.main[i],
            Slot::Craft(i) => inventory.crafting[i],
            Slot::Armor(i) => inventory.armor[i],
        };
        let has_icon = stack
            .and_then(|stack| block_icons.rect_for_stack(stack))
            .is_some();
        node.right = if has_icon { px(2.0) } else { Val::Auto };
        node.bottom = if has_icon { px(0.0) } else { Val::Auto };
        node.left = if has_icon { Val::Auto } else { px(0.0) };
        node.top = if has_icon { Val::Auto } else { px(0.0) };
        **text = if has_icon {
            stack
                .map(|s| {
                    if s.count() > 1 {
                        s.count().to_string()
                    } else {
                        String::new()
                    }
                })
                .unwrap_or_default()
        } else {
            stack_text(stack)
        };
    }
    for (bar, mut node, mut visibility, mut color) in &mut bars {
        let stack = match bar.0 {
            Slot::Hotbar(i) => hotbar.slots[i],
            Slot::Main(i) => inventory.main[i],
            Slot::Craft(i) => inventory.crafting[i],
            Slot::Armor(i) => inventory.armor[i],
        };
        if let Some((width, red, green)) = stack.and_then(durability_bar) {
            *visibility = Visibility::Inherited;
            if bar.1 {
                node.width = px(width);
                *color = BackgroundColor(Color::srgb_u8(red, green, 0));
            } else {
                node.width = px(26.0);
                *color = BackgroundColor(Color::BLACK);
            }
        } else {
            *visibility = Visibility::Hidden;
        }
    }
    for (icon, mut image, mut visibility) in &mut icons {
        let stack = match icon.0 {
            Slot::Hotbar(i) => hotbar.slots[i],
            Slot::Main(i) => inventory.main[i],
            Slot::Craft(i) => inventory.crafting[i],
            Slot::Armor(i) => inventory.armor[i],
        };
        if let Some(rect) = stack.and_then(|stack| block_icons.rect_for_stack(stack)) {
            image.rect = Some(rect);
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
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
        if let Ok(window) = windows.single() {
            if let Some(pos) = window.cursor_position() {
                node.left = px(pos.x - (window.width() - 176.0 * SCALE) / 2.0 + 8.0);
                node.top = px(pos.y - (window.height() - 166.0 * SCALE) / 2.0 + 8.0);
            }
        }
    }
    if let Ok((mut text, mut node)) = carried.single_mut() {
        **text = if block_icons.ready()
            && inventory
                .carried
                .and_then(|stack| block_icons.rect_for_stack(stack))
                .is_some()
        {
            inventory
                .carried
                .map(|s| {
                    if s.count() > 1 {
                        s.count().to_string()
                    } else {
                        String::new()
                    }
                })
                .unwrap_or_default()
        } else {
            stack_text(inventory.carried)
        };
        if let Ok(window) = windows.single() {
            if let Some(pos) = window.cursor_position() {
                node.left = px(pos.x - (window.width() - 176.0 * SCALE) / 2.0 + 8.0);
                node.top = px(pos.y - (window.height() - 166.0 * SCALE) / 2.0 + 8.0);
            }
        }
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
