//! Opening and closing the screen, and building its panels and slots.

use super::CarriedIcon;
use super::CarriedLabel;
use super::FurnaceProgress;
use super::InventoryPanel;
use super::InventoryRoot;
use super::InventoryTexture;
use super::SLOT_SIZE;
use super::SLOT_STEP;
use super::Slot;
use super::SlotDurability;
use super::SlotHighlight;
use super::SlotIcon;
use super::SlotLabel;
use crate::app::settings::GameSettings;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::inventory::session::close_crafting_session;
use crate::player::Player;
use crate::random::ItemRng;
use crate::rendering::icons::BlockIcons;
use crate::ui::icons::overlay::UiFont;
use crate::ui::icons::overlay::count_frame;
use crate::ui::icons::overlay::count_line_height;
use crate::ui::icons::overlay::count_shadow;
use crate::ui::icons::overlay::count_text_font;
use crate::ui::icons::overlay::durability_track;
use crate::ui::icons::overlay::icon_size;
use crate::world::chunk::ChestGroup;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::text::Justify;
use bevy::text::TextLayout;
use bevy::ui::RelativeCursorPosition;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

pub(super) fn load_texture(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(InventoryTexture {
        background: assets.load("gui/inventory.png"),
        crafting: assets.load("gui/crafting.png"),
        furnace: assets.load("gui/furnace.png"),
        container: assets.load("gui/container.png"),
    });
}

pub(super) fn toggle(
    chat: Option<Res<crate::chat::ChatFocus>>,
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut screen: ResMut<InventorySession>,
    mut workbench: ResMut<ActiveWorkbench>,
    texture: Res<InventoryTexture>,
    icons: Res<BlockIcons>,
    font: Res<UiFont>,
    settings: Res<GameSettings>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut item_rng: Local<ItemRng>,
) {
    if chat.is_some_and(|chat| chat.suppress_controls) {
        return;
    }
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
            settings.gui_scale,
            background,
            &icons.image,
            &font.minecraft,
            screen.workbench,
            screen.furnace,
            screen
                .chest_group
                .map_or(0, ChestGroup::slot_count)
                .div_ceil(9),
            screen.cart.is_some(),
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
            close_crafting_session(
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
        screen.cart = None;
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
            settings.gui_scale,
            background,
            &icons.image,
            &font.minecraft,
            screen.workbench,
            screen.furnace,
            screen
                .chest_group
                .map_or(0, ChestGroup::slot_count)
                .div_ceil(9),
            screen.cart.is_some(),
            &texture.container,
        );
    } else {
        for root in &roots {
            commands.entity(root).despawn();
        }
    }
}

pub(super) fn close(
    mut commands: Commands,
    mut screen: ResMut<InventorySession>,
    mut workbench: ResMut<ActiveWorkbench>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>,
    mut item_rng: Local<ItemRng>,
) {
    if let Ok((transform, mut hotbar, mut inventory)) = player.single_mut() {
        close_crafting_session(
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
    screen.cart = None;
    for root in &roots {
        commands.entity(root).despawn();
    }
}

pub(super) fn spawn(
    commands: &mut Commands,
    scale: f32,
    texture: &Handle<Image>,
    icons: &Handle<Image>,
    font: &Handle<Font>,
    workbench: bool,
    furnace: bool,
    chest_rows: usize,
    cart: bool,
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
                InventoryPanel,
                RelativeCursorPosition::default(),
                Node {
                    position_type: PositionType::Relative,
                    width: px(176.0 * scale),
                    height: px(panel_height * scale),
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
                            width: px(176.0 * scale),
                            height: px(chest_top_height * scale),
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
                            top: px(chest_top_height * scale),
                            width: px(176.0 * scale),
                            height: px(96.0 * scale),
                            ..default()
                        },
                    ));
                    chest_panel_label(
                        panel,
                        scale,
                        font,
                        if chest_rows == 1 {
                            "Dispenser"
                        } else if cart {
                            "Minecart"
                        } else if chest_rows == 3 {
                            "Chest"
                        } else {
                            "Large chest"
                        },
                        8.0,
                        6.0,
                    );
                    chest_panel_label(panel, scale, font, "Inventory", 8.0, chest_top_height + 3.0);
                    for index in 0..chest_rows * 9 {
                        let x = 8.0 + (index % 9) as f32 * SLOT_STEP;
                        let y = 18.0 + (index / 9) as f32 * SLOT_STEP;
                        slot(panel, scale, icons, font, Slot::Chest(index), x, y);
                    }
                    for index in 0..27 {
                        let x = 8.0 + (index % 9) as f32 * SLOT_STEP;
                        let y = player_top + (index / 9) as f32 * SLOT_STEP;
                        slot(panel, scale, icons, font, Slot::Main(index), x, y);
                    }
                    for index in 0..9 {
                        let x = 8.0 + index as f32 * SLOT_STEP;
                        slot(
                            panel,
                            scale,
                            icons,
                            font,
                            Slot::Hotbar(index),
                            x,
                            hotbar_top,
                        );
                    }
                } else {
                    panel.spawn((
                        ImageNode::new(texture.clone())
                            .with_rect(Rect::new(0.0, 0.0, 176.0, 166.0)),
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(0.0),
                            width: px(176.0 * scale),
                            height: px(166.0 * scale),
                            ..default()
                        },
                    ));
                    for index in 0..27 {
                        slot(
                            panel,
                            scale,
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
                            scale,
                            icons,
                            font,
                            Slot::Hotbar(index),
                            8.0 + index as f32 * SLOT_STEP,
                            142.0,
                        );
                    }
                    if furnace {
                        slot(panel, scale, icons, font, Slot::Furnace(0), 56.0, 17.0);
                        slot(panel, scale, icons, font, Slot::Furnace(1), 56.0, 53.0);
                        slot(panel, scale, icons, font, Slot::Furnace(2), 116.0, 35.0);
                        panel.spawn((
                            FurnaceProgress(false),
                            Pickable::IGNORE,
                            Visibility::Hidden,
                            ImageNode::new(texture.clone())
                                .with_rect(Rect::new(176.0, 0.0, 190.0, 14.0)),
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(56.0 * scale),
                                top: px(50.0 * scale),
                                width: px(14.0 * scale),
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
                                left: px(79.0 * scale),
                                top: px(34.0 * scale),
                                width: px(24.0 * scale),
                                height: px(16.0 * scale),
                                ..default()
                            },
                        ));
                    } else if workbench {
                        for index in 0..9 {
                            slot(
                                panel,
                                scale,
                                icons,
                                font,
                                Slot::Workbench(index),
                                30.0 + (index % 3) as f32 * SLOT_STEP,
                                17.0 + (index / 3) as f32 * SLOT_STEP,
                            );
                        }
                        slot(panel, scale, icons, font, Slot::CraftResult, 124.0, 35.0);
                    } else {
                        for index in 0..4 {
                            slot(
                                panel,
                                scale,
                                icons,
                                font,
                                Slot::Craft(index),
                                88.0 + (index % 2) as f32 * SLOT_STEP,
                                26.0 + (index / 2) as f32 * SLOT_STEP,
                            );
                        }
                        slot(panel, scale, icons, font, Slot::CraftResult, 144.0, 36.0);
                    }
                    for index in 0..4 {
                        slot(
                            panel,
                            scale,
                            icons,
                            font,
                            Slot::Armor(index),
                            8.0,
                            8.0 + index as f32 * SLOT_STEP,
                        );
                    }
                }
                let carried = count_frame(scale, 0.0, 0.0);
                panel.spawn((
                    CarriedIcon,
                    Pickable::IGNORE,
                    Visibility::Hidden,
                    ImageNode::new(icons.clone()),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(0),
                        width: px(icon_size(scale)),
                        height: px(icon_size(scale)),
                        ..default()
                    },
                ));
                panel.spawn((
                    CarriedLabel,
                    Pickable::IGNORE,
                    Text::new(""),
                    count_text_font(scale, font),
                    TextLayout::justify(Justify::Right),
                    count_line_height(scale),
                    TextColor(Color::WHITE),
                    count_shadow(scale),
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

pub(super) fn chest_panel_label(
    panel: &mut ChildSpawnerCommands,
    scale: f32,
    font: &Handle<Font>,
    label: &str,
    x: f32,
    y: f32,
) {
    panel.spawn((
        Text::new(label),
        TextFont {
            font: FontSource::Handle(font.clone()),
            font_size: FontSize::Px(8.0 * scale),
            ..default()
        },
        TextColor(Color::srgb_u8(64, 64, 64)),
        Pickable::IGNORE,
        Node {
            position_type: PositionType::Absolute,
            left: px(x * scale),
            top: px(y * scale),
            ..default()
        },
    ));
}

pub(super) fn slot(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    scale: f32,
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
                left: px(x * scale),
                top: px(y * scale),
                width: px(SLOT_SIZE * scale),
                height: px(SLOT_SIZE * scale),
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
                    width: px(icon_size(scale)),
                    height: px(icon_size(scale)),
                    ..default()
                },
            ));
            for foreground in [false, true] {
                let (bar_left, bar_top, bar_width, bar_height) =
                    durability_track(scale, 0.0, 0.0, foreground);
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
            let frame = count_frame(scale, 0.0, 0.0);
            button.spawn((
                SlotLabel(id),
                Pickable::IGNORE,
                Text::new(""),
                count_text_font(scale, font),
                TextLayout::justify(Justify::Right),
                count_line_height(scale),
                TextColor(Color::WHITE),
                count_shadow(scale),
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
