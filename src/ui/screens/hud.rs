use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::math::Rect;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::text::FontSource;
use bevy::text::LineHeight;

use super::inventory::durability_bar;
use crate::app::state::AppScreen;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Flying;
use crate::entity::drops::items::hotbar_icon_scale;
use crate::inventory::HOTBAR_SLOTS;
use crate::inventory::Hotbar;
use crate::item::ItemStack;
use crate::physics::BLOCK_REACH;
use crate::physics::raycast_blocks;
use crate::player::HeartFill;
use crate::player::Player;
use crate::player::PlayerCamera;
use crate::player::PlayerHealth;
use crate::ui::icons::blocks::BlockIcons;
use crate::ui::icons::overlay::GUI_SCALE;
use crate::ui::icons::overlay::UiFont;
use crate::ui::icons::overlay::count_label;
use crate::ui::icons::overlay::durability_track;
use crate::ui::icons::overlay::icon_size;
use crate::ui::icons::overlay::place_stack_label;
use crate::ui::icons::overlay::sync_stack_label;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::tick::WorldTick;

const HUD_SCALE: f32 = GUI_SCALE;
const HEART_COUNT: usize = 10;
const HOTBAR_WIDTH: f32 = 182.0;
const HOTBAR_HEIGHT: f32 = 22.0;
const SELECTOR_WIDTH: f32 = 24.0;
const SELECTOR_HEIGHT: f32 = 22.0;
const CROSSHAIR_SIZE: f32 = 16.0;
const HEART_SIZE: f32 = 9.0;
const HEART_STRIDE: f32 = 8.0;
const HOTBAR_SLOT_STEP: f32 = 20.0;
const HOTBAR_ICON_INSET: f32 = 3.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugVisible>()
            .add_systems(PreStartup, load_hud_textures)
            .add_systems(OnEnter(AppScreen::Playing), spawn_hud)
            .add_systems(OnExit(AppScreen::Playing), despawn_hud)
            .add_systems(
                Update,
                (
                    update_hearts,
                    update_hotbar_selector,
                    update_hotbar_items,
                    update_hotbar_icons,
                    update_hotbar_bars,
                    update_debug_overlay,
                )
                    .run_if(in_state(AppScreen::Playing)),
            );
    }
}

#[derive(Resource)]
struct HudTextures {
    widgets: Handle<Image>,
    icons: Handle<Image>,
}

#[derive(Component)]
struct HudRoot;

#[derive(Resource, Default)]
struct DebugVisible(bool);

#[derive(Component)]
struct DebugOverlay;

#[derive(Component)]
struct HudHeart(usize);

#[derive(Component)]
struct HotbarSelector;
#[derive(Component)]
struct HotbarItem(usize);
#[derive(Component)]
struct HotbarBlockIcon(usize);
#[derive(Component)]
struct HotbarDurability(usize, bool);

fn load_hud_textures(mut commands: Commands, asset_server: Res<AssetServer>) {
    crate::ui::icons::overlay::load_ui_font(&mut commands, &asset_server);
    let load = |path| asset_server.load(path);
    commands.insert_resource(HudTextures {
        widgets: load("gui/gui.png"),
        icons: load("gui/icons.png"),
    });
}

fn spawn_hud(
    mut commands: Commands,
    textures: Res<HudTextures>,
    icons: Res<BlockIcons>,
    font: Res<UiFont>,
    debug_visible: Res<DebugVisible>,
    player: Query<(&PlayerHealth, &Hotbar), With<Player>>,
) {
    let Ok((health, hotbar)) = player.single() else {
        return;
    };

    commands
        .spawn((
            HudRoot,
            Pickable::IGNORE,
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
        ))
        .with_children(|root| {
            spawn_crosshair(root, &textures);
            spawn_status(root, &textures, &icons, &font.minecraft, health, hotbar);
            root.spawn((
                DebugOverlay,
                Pickable::IGNORE,
                Text::new(""),
                TextFont {
                    font: FontSource::Handle(font.minecraft.clone()),
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
                if debug_visible.0 {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(8),
                    top: px(8),
                    padding: UiRect::all(px(6)),
                    ..default()
                },
            ));
        });
}

fn update_debug_overlay(
    chat: Option<Res<crate::ui::ChatState>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut visible: ResMut<DebugVisible>,
    player: Query<(&Transform, &CollisionState, Option<&Flying>), With<Player>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    chunks: Res<WorldChunks>,
    mut overlay: Query<(&mut Text, &mut Visibility), With<DebugOverlay>>,
) {
    if !chat.is_some_and(|chat| chat.suppress_controls)
        && keys.is_some_and(|keys| keys.just_pressed(KeyCode::F3))
    {
        visible.0 = !visible.0;
    }
    let Ok((mut text, mut visibility)) = overlay.single_mut() else {
        return;
    };
    visibility.set_if_neq(if visible.0 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    });
    if !visible.0 {
        return;
    }
    let Ok((transform, collision, flying)) = player.single() else {
        return;
    };
    let p = transform.translation;
    let feet_y = p.y - EntitySize::PLAYER.y_offset;
    let (bx, by, bz) = (
        p.x.floor() as i32,
        feet_y.floor() as i32,
        p.z.floor() as i32,
    );
    let below = chunks
        .block_at(bx, (feet_y - 0.01).floor() as i32, bz)
        .map_or_else(|| "unloaded".to_string(), |block| format!("{block:?}"));
    let chunk = ChunkPosition::from_block(bx, bz);
    let target = camera
        .single()
        .ok()
        .and_then(|camera| {
            raycast_blocks(
                &chunks,
                camera.translation(),
                *camera.forward(),
                BLOCK_REACH,
            )
        })
        .map_or_else(
            || "none".to_string(),
            |hit| format!("{:?} at {} / {} / {}", hit.block, hit.x, hit.y, hit.z),
        );
    let flight = if flying.is_some() { "ON" } else { "OFF" };
    let next = format!(
        "XYZ: {:.2} / {:.2} / {:.2}\nBlock: {} / {} / {}  Chunk: {} / {}\nFeet Y: {:.2}  Grounded: {}  Below: {}\nFlight: {}  Target: {}",
        p.x,
        p.y,
        p.z,
        bx,
        by,
        bz,
        chunk.x,
        chunk.z,
        feet_y,
        collision.on_ground,
        below,
        flight,
        target
    );
    if text.0 != next {
        text.0 = next;
    }
}

fn spawn_crosshair(parent: &mut ChildSpawnerCommands, textures: &HudTextures) {
    let size = CROSSHAIR_SIZE * HUD_SCALE;
    parent.spawn((
        Pickable::IGNORE,
        ImageNode::new(textures.icons.clone()).with_rect(Rect::new(0.0, 0.0, 16.0, 16.0)),
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            width: px(size),
            height: px(size),
            margin: UiRect::axes(px(-size / 2.0), px(-size / 2.0)),
            ..default()
        },
    ));
}

fn spawn_status(
    parent: &mut ChildSpawnerCommands,
    textures: &HudTextures,
    icons: &BlockIcons,
    font: &Handle<Font>,
    health: &PlayerHealth,
    hotbar: &Hotbar,
) {
    parent
        .spawn((
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(HUD_SCALE),
                ..default()
            },
        ))
        .with_children(|bottom| {
            spawn_hearts(bottom, textures, health);
            spawn_hotbar(bottom, textures, icons, font, hotbar);
        });
}

fn spawn_hearts(parent: &mut ChildSpawnerCommands, textures: &HudTextures, health: &PlayerHealth) {
    parent
        .spawn((
            Pickable::IGNORE,
            Node {
                width: px(HOTBAR_WIDTH * HUD_SCALE),
                height: px(HEART_SIZE * HUD_SCALE),
                ..default()
            },
        ))
        .with_children(|row| {
            for index in 0..HEART_COUNT {
                let left = px(index as f32 * HEART_STRIDE * HUD_SCALE);
                let size = px(HEART_SIZE * HUD_SCALE);
                row.spawn((
                    Pickable::IGNORE,
                    ImageNode::new(textures.icons.clone()).with_rect(heart_container_rect()),
                    Node {
                        position_type: PositionType::Absolute,
                        left,
                        width: size,
                        height: size,
                        ..default()
                    },
                ));
                let fill = health.heart_fill(index);
                let mut heart = row.spawn((
                    HudHeart(index),
                    Pickable::IGNORE,
                    ImageNode::new(textures.icons.clone()).with_rect(heart_fill_rect(fill)),
                    Node {
                        position_type: PositionType::Absolute,
                        left,
                        width: size,
                        height: size,
                        ..default()
                    },
                ));
                if fill == HeartFill::Empty {
                    heart.insert(Visibility::Hidden);
                }
            }
        });
}

fn spawn_hotbar(
    parent: &mut ChildSpawnerCommands,
    textures: &HudTextures,
    icons: &BlockIcons,
    font: &Handle<Font>,
    hotbar: &Hotbar,
) {
    parent
        .spawn((
            Pickable::IGNORE,
            ImageNode::new(textures.widgets.clone()).with_rect(Rect::new(0.0, 0.0, 182.0, 22.0)),
            Node {
                width: px(HOTBAR_WIDTH * HUD_SCALE),
                height: px(HOTBAR_HEIGHT * HUD_SCALE),
                ..default()
            },
        ))
        .with_children(|bar| {
            bar.spawn((
                HotbarSelector,
                Pickable::IGNORE,
                ImageNode::new(textures.widgets.clone())
                    .with_rect(Rect::new(0.0, 22.0, 24.0, 44.0)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(hotbar_selector_left(hotbar.selected)),
                    top: px(-HUD_SCALE),
                    width: px(SELECTOR_WIDTH * HUD_SCALE),
                    height: px(SELECTOR_HEIGHT * HUD_SCALE),
                    ..default()
                },
            ));
            for (index, stack) in hotbar.slots.iter().copied().enumerate() {
                spawn_hotbar_item(bar, icons, font, index, stack);
            }
        });
}

/// Slot position for a 16×16 item icon inside the hotbar texture.
fn hotbar_item_rect(index: usize) -> (f32, f32) {
    (
        (HOTBAR_ICON_INSET + index as f32 * HOTBAR_SLOT_STEP) * HUD_SCALE,
        HOTBAR_ICON_INSET * HUD_SCALE,
    )
}

fn spawn_hotbar_item(
    parent: &mut ChildSpawnerCommands,
    icons: &BlockIcons,
    font: &Handle<Font>,
    index: usize,
    stack: Option<ItemStack>,
) {
    let (left, top) = hotbar_item_rect(index);
    parent.spawn((
        HotbarBlockIcon(index),
        Pickable::IGNORE,
        Visibility::Hidden,
        ImageNode::new(icons.image.clone()),
        Node {
            position_type: PositionType::Absolute,
            left: px(left),
            top: px(top),
            width: px(icon_size()),
            height: px(icon_size()),
            ..default()
        },
    ));
    for foreground in [false, true] {
        let (bar_left, bar_top, bar_width, bar_height) = durability_track(left, top, foreground);
        parent.spawn((
            HotbarDurability(index, foreground),
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
    let has_icon = stack
        .and_then(|stack| icons.rect_for_stack(stack))
        .is_some();
    let label = if has_icon {
        count_label(stack)
    } else {
        hotbar_label(stack)
    };
    let mut text = Text::new(label.clone());
    let mut node = Node::default();
    let mut layout = TextLayout::default();
    let mut text_font = TextFont::default();
    let mut line_height = LineHeight::default();
    let mut shadow = TextShadow::default();
    place_stack_label(
        &mut text,
        &mut node,
        &mut layout,
        &mut text_font,
        &mut line_height,
        &mut shadow,
        font,
        left,
        top,
        &label,
        has_icon,
    );
    parent.spawn((
        HotbarItem(index),
        Pickable::IGNORE,
        text,
        text_font,
        layout,
        line_height,
        TextColor(Color::WHITE),
        shadow,
        node,
    ));
}

fn hotbar_label(stack: Option<ItemStack>) -> String {
    stack
        .map(|stack| {
            let label = stack.item().to_string();
            let short = &label[..label.len().min(5)];
            if stack.count() > 1 {
                format!("{short}\n{}", stack.count())
            } else {
                short.to_string()
            }
        })
        .unwrap_or_default()
}

fn despawn_hud(mut commands: Commands, roots: Query<Entity, With<HudRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

fn update_hearts(
    health: Query<&PlayerHealth, (With<Player>, Changed<PlayerHealth>)>,
    mut hearts: Query<(&HudHeart, &mut ImageNode, &mut Visibility)>,
) {
    let Ok(health) = health.single() else {
        return;
    };
    for (heart, mut image, mut visibility) in &mut hearts {
        let fill = health.heart_fill(heart.0);
        image.rect = Some(heart_fill_rect(fill));
        *visibility = if fill == HeartFill::Empty {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

fn update_hotbar_selector(
    hotbar: Query<&Hotbar, (With<Player>, Changed<Hotbar>)>,
    mut selector: Query<&mut Node, With<HotbarSelector>>,
) {
    let Ok(hotbar) = hotbar.single() else {
        return;
    };
    let Ok(mut node) = selector.single_mut() else {
        return;
    };
    node.left = px(hotbar_selector_left(hotbar.selected));
}

fn update_hotbar_items(
    hotbar: Query<Ref<Hotbar>, With<Player>>,
    mut labels: Query<(
        &HotbarItem,
        &mut Text,
        &mut Node,
        &mut TextLayout,
        &mut TextFont,
        &mut LineHeight,
        &mut TextShadow,
    )>,
    icons: Res<BlockIcons>,
    font: Res<UiFont>,
) {
    let Ok(hotbar) = hotbar.single() else {
        return;
    };
    if !hotbar.is_changed() && !icons.is_changed() {
        return;
    }
    for (item, mut text, mut node, mut layout, mut text_font, mut line_height, mut shadow) in
        &mut labels
    {
        let stack = hotbar.slots[item.0];
        let has_icon = stack
            .and_then(|stack| icons.rect_for_stack(stack))
            .is_some();
        let label = if has_icon {
            count_label(stack)
        } else {
            hotbar_label(stack)
        };
        let (left, top) = hotbar_item_rect(item.0);
        sync_stack_label(
            &mut text,
            &mut node,
            &mut layout,
            &mut text_font,
            &mut line_height,
            &mut shadow,
            &font.minecraft,
            left,
            top,
            &label,
            has_icon,
        );
    }
}

fn update_hotbar_icons(
    hotbar: Query<&Hotbar, With<Player>>,
    icons: Res<BlockIcons>,
    clock: Option<Res<WorldTick>>,
    mut images: Query<(&HotbarBlockIcon, &mut ImageNode, &mut Visibility, &mut Node)>,
) {
    let Ok(hotbar) = hotbar.single() else {
        return;
    };
    let partial = clock.map(|clock| clock.partial()).unwrap_or(0.0);
    for (slot, mut image, mut visibility, mut node) in &mut images {
        if let Some(rect) = hotbar.slots[slot.0].and_then(|stack| icons.rect_for_stack(stack)) {
            image
                .reborrow()
                .map_unchanged(|image| &mut image.rect)
                .set_if_neq(Some(rect));
            visibility.set_if_neq(Visibility::Inherited);
            let scale = hotbar_icon_scale(hotbar.pop[slot.0], partial);
            let width = icon_size() * scale.x;
            let height = icon_size() * scale.y;
            let (left, top) = hotbar_item_rect(slot.0);
            let mut next = node.clone();
            next.left = px(left + (icon_size() - width) * 0.5);
            next.top = px(top + (icon_size() - height) * 0.5);
            next.width = px(width);
            next.height = px(height);
            node.set_if_neq(next);
        } else {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

fn update_hotbar_bars(
    hotbar: Query<&Hotbar, With<Player>>,
    mut bars: Query<(
        &HotbarDurability,
        &mut Node,
        &mut Visibility,
        &mut BackgroundColor,
    )>,
) {
    let Ok(hotbar) = hotbar.single() else {
        return;
    };
    for (bar, mut node, mut visibility, mut color) in &mut bars {
        if let Some((width, red, green)) = hotbar.slots[bar.0].and_then(durability_bar) {
            visibility.set_if_neq(Visibility::Inherited);
            let (_, _, track_width, _) = durability_track(0.0, 0.0, bar.1);
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
}

fn hotbar_selector_left(selected: usize) -> f32 {
    (selected.min(HOTBAR_SLOTS - 1) as f32 * HOTBAR_SLOT_STEP - 1.0) * HUD_SCALE
}

fn heart_container_rect() -> Rect {
    Rect::new(16.0, 0.0, 25.0, 9.0)
}

fn heart_fill_rect(fill: HeartFill) -> Rect {
    match fill {
        HeartFill::Empty | HeartFill::Full => Rect::new(52.0, 0.0, 61.0, 9.0),
        HeartFill::Half => Rect::new(61.0, 0.0, 70.0, 9.0),
    }
}
