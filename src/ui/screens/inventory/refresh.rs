//! Redraws slot icons, counts, durability bars and furnace progress from
//! the inventory each frame something changed.

use super::CarriedIcon;
use super::CarriedLabel;
use super::FurnaceProgress;
use super::Slot;
use super::SlotDrag;
use super::SlotDurability;
use super::SlotHighlight;
use super::SlotIcon;
use super::SlotLabel;
use super::chest::read_chest_group_slots;
use super::clicks::to_slot_id;
use crate::app::settings::GameSettings;
use crate::crafting::beta_recipe_book;
use crate::inventory::DragPlace;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::SlotId;
use crate::inventory::preview_chest_drag_place;
use crate::inventory::preview_drag_place;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::item::ItemData;
use crate::item::ItemStack;
use crate::player::Player;
use crate::rendering::icons::BlockIcons;
use crate::ui::icons::overlay::UiFont;
use crate::ui::icons::overlay::count_label;
use crate::ui::icons::overlay::durability_track;
use crate::ui::icons::overlay::sync_stack_label;
use crate::world::chunk::ChestGroup;
use crate::world::chunk::WorldChunks;
use bevy::prelude::*;
use bevy::text::LineHeight;
use bevy::text::TextLayout;
use bevy::ui::RelativeCursorPosition;
use bevy::window::PrimaryWindow;

pub(super) fn highlight_slots(
    screen: Res<InventorySession>,
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

pub(super) fn stack_text(stack: Option<ItemStack>) -> String {
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

pub(super) fn refresh(
    screen: Res<InventorySession>,
    player: Query<(&Hotbar, &Inventory), With<Player>>,
    workbench: Res<ActiveWorkbench>,
    chunks: Res<WorldChunks>,
    drag: Res<SlotDrag>,
    mut labels: Query<
        (
            Entity,
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
    settings: Res<GameSettings>,
    mut label_cache: Local<std::collections::HashMap<Entity, (Option<ItemStack>, bool)>>,
) {
    let scale = settings.gui_scale;
    if !screen.open {
        label_cache.clear();
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
    for (
        entity,
        label,
        mut text,
        mut node,
        mut layout,
        mut text_font,
        mut line_height,
        mut shadow,
    ) in &mut labels
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
        let displayed = (stack, has_icon);
        if label_cache.get(&entity) == Some(&displayed) {
            continue;
        }
        label_cache.insert(entity, displayed);
        let label_text = if has_icon {
            count_label(stack)
        } else {
            stack_text(stack)
        };
        sync_stack_label(
            scale,
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
        if let Some((width, red, green)) = stack.and_then(|stack| durability_bar(scale, stack)) {
            visibility.set_if_neq(Visibility::Inherited);
            let (_, _, track_width, _) = durability_track(scale, 0.0, 0.0, bar.1);
            if bar.1 {
                node.reborrow()
                    .map_unchanged(|node| &mut node.width)
                    .set_if_neq(px(width));
                color.set_if_neq(BackgroundColor(Color::srgb_u8(red, green, 0)));
            } else {
                node.reborrow()
                    .map_unchanged(|node| &mut node.width)
                    .set_if_neq(px(track_width));
                color.set_if_neq(BackgroundColor(Color::BLACK));
            }
        } else {
            visibility.set_if_neq(Visibility::Hidden);
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
            image
                .reborrow()
                .map_unchanged(|image| &mut image.rect)
                .set_if_neq(Some(rect));
            visibility.set_if_neq(Visibility::Inherited);
        } else {
            visibility.set_if_neq(Visibility::Hidden);
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
            visibility.set_if_neq(Visibility::Hidden);
        } else {
            visibility.set_if_neq(Visibility::Inherited);
            if indicator.0 {
                let width = (24.0 * progress).ceil().clamp(1.0, 24.0);
                image
                    .reborrow()
                    .map_unchanged(|image| &mut image.rect)
                    .set_if_neq(Some(Rect::new(176.0, 14.0, 176.0 + width, 30.0)));
                node.reborrow()
                    .map_unchanged(|node| &mut node.width)
                    .set_if_neq(px(width * scale));
                node.reborrow()
                    .map_unchanged(|node| &mut node.height)
                    .set_if_neq(px(16.0 * scale));
                node.reborrow()
                    .map_unchanged(|node| &mut node.left)
                    .set_if_neq(px(79.0 * scale));
                node.reborrow()
                    .map_unchanged(|node| &mut node.top)
                    .set_if_neq(px(34.0 * scale));
            } else {
                let height = (14.0 * progress).ceil().clamp(1.0, 14.0);
                image
                    .reborrow()
                    .map_unchanged(|image| &mut image.rect)
                    .set_if_neq(Some(Rect::new(176.0, 14.0 - height, 190.0, 14.0)));
                node.reborrow()
                    .map_unchanged(|node| &mut node.height)
                    .set_if_neq(px(height * scale));
                node.reborrow()
                    .map_unchanged(|node| &mut node.top)
                    .set_if_neq(px((50.0 - height) * scale));
                node.reborrow()
                    .map_unchanged(|node| &mut node.left)
                    .set_if_neq(px(56.0 * scale));
                node.reborrow()
                    .map_unchanged(|node| &mut node.width)
                    .set_if_neq(px(14.0 * scale));
            }
        }
    }
    let cursor_icon = windows.single().ok().and_then(|window| {
        window.cursor_position().map(|pos| {
            // GuiContainer draws the carried stack at the cursor minus half an icon.
            (
                pos.x - (window.width() - 176.0 * scale) / 2.0 - 8.0 * scale,
                pos.y
                    - (window.height()
                        - (if screen.chest {
                            114.0
                                + screen.chest_group.map_or(3, |group| group.slot_count() / 9)
                                    as f32
                                    * 18.0
                        } else {
                            166.0
                        }) * scale)
                        / 2.0
                    - 8.0 * scale,
            )
        })
    });
    if let Ok((mut image, mut visibility, mut node)) = carried_icon.single_mut() {
        if let Some(rect) = inventory
            .carried
            .and_then(|stack| block_icons.rect_for_stack(stack))
        {
            image
                .reborrow()
                .map_unchanged(|image| &mut image.rect)
                .set_if_neq(Some(rect));
            visibility.set_if_neq(Visibility::Inherited);
        } else {
            visibility.set_if_neq(Visibility::Hidden);
        }
        if let Some((left, top)) = cursor_icon {
            node.reborrow()
                .map_unchanged(|node| &mut node.left)
                .set_if_neq(px(left));
            node.reborrow()
                .map_unchanged(|node| &mut node.top)
                .set_if_neq(px(top));
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
            sync_stack_label(
                scale,
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
            if text.0 != label_text {
                text.0 = label_text;
            }
        }
    }
}

pub(super) fn displayed_stack(
    slot: Slot,
    hotbar: &Hotbar,
    inventory: &Inventory,
    workbench: &ActiveWorkbench,
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

pub(super) fn slot_stack(
    slot: Slot,
    hotbar: &Hotbar,
    inventory: &Inventory,
    workbench: &ActiveWorkbench,
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

pub(crate) fn durability_bar(scale: f32, stack: ItemStack) -> Option<(f32, u8, u8)> {
    let ItemData::Durability(max) = stack.definition().data else {
        return None;
    };
    if stack.data() == 0 || max == 0 {
        return None;
    }
    let fraction = stack.data() as f32 / max as f32;
    let width = (13.0 - fraction * 13.0).round().clamp(0.0, 13.0) * scale;
    let green = (255.0 - fraction * 255.0).round().clamp(0.0, 255.0) as u8;
    Some((width, 255 - green, green))
}
