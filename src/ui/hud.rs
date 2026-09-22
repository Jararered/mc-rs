use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::math::Rect;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;

use super::block_icons::BlockIcons;
use super::inventory::durability_bar;
use crate::app::state::AppScreen;
use crate::inventory::HOTBAR_SLOTS;
use crate::inventory::Hotbar;
use crate::item::ItemStack;
use crate::player::HeartFill;
use crate::player::Player;
use crate::player::PlayerHealth;

const HUD_SCALE: f32 = 2.0;
const HEART_COUNT: usize = 10;
const HOTBAR_WIDTH: f32 = 182.0;
const HOTBAR_HEIGHT: f32 = 22.0;
const SELECTOR_WIDTH: f32 = 24.0;
const SELECTOR_HEIGHT: f32 = 22.0;
const CROSSHAIR_SIZE: f32 = 16.0;
const HEART_SIZE: f32 = 9.0;
const HEART_STRIDE: f32 = 8.0;
const ITEM_SIZE: f32 = 16.0;
const HOTBAR_SLOT_STEP: f32 = 20.0;
const HOTBAR_ICON_INSET: f32 = 2.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_hud_textures)
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
    let load = |path| {
        asset_server
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::nearest();
            })
            .load(path)
    };
    commands.insert_resource(HudTextures {
        widgets: load("gui/gui.png"),
        icons: load("gui/icons.png"),
    });
}

fn spawn_hud(
    mut commands: Commands,
    textures: Res<HudTextures>,
    icons: Res<BlockIcons>,
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
            spawn_status(root, &textures, &icons, health, hotbar);
        });
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
            spawn_hotbar(bottom, textures, icons, hotbar);
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
                spawn_hotbar_item(bar, icons, index, stack);
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
            width: px(ITEM_SIZE * HUD_SCALE),
            height: px(ITEM_SIZE * HUD_SCALE),
            ..default()
        },
    ));
    for (foreground, height, top_offset) in [(false, 4.0, 26.0), (true, 2.0, 28.0)] {
        parent.spawn((
            HotbarDurability(index, foreground),
            Pickable::IGNORE,
            Visibility::Hidden,
            BackgroundColor(Color::BLACK),
            Node {
                position_type: PositionType::Absolute,
                left: px(left + 4.0),
                top: px(top + top_offset),
                width: px(26.0),
                height: px(height),
                ..default()
            },
        ));
    }
    let has_icon = stack
        .and_then(|stack| icons.rect_for_stack(stack))
        .is_some();
    parent.spawn((
        HotbarItem(index),
        Pickable::IGNORE,
        Text::new(visible_label(stack, icons)),
        TextFont {
            font_size: 10.0.into(),
            ..default()
        },
        TextColor(Color::WHITE),
        TextShadow::default(),
        Node {
            position_type: PositionType::Absolute,
            left: px(if has_icon { left + 14.0 } else { left }),
            top: px(if has_icon { top + 16.0 } else { top }),
            width: px(if has_icon {
                18.0
            } else {
                ITEM_SIZE * HUD_SCALE
            }),
            height: px(ITEM_SIZE * HUD_SCALE),
            ..default()
        },
    ));
}

fn visible_label(stack: Option<ItemStack>, icons: &BlockIcons) -> String {
    if stack
        .and_then(|stack| icons.rect_for_stack(stack))
        .is_some()
    {
        stack
            .map(|stack| {
                if stack.count() > 1 {
                    stack.count().to_string()
                } else {
                    String::new()
                }
            })
            .unwrap_or_default()
    } else {
        hotbar_label(stack)
    }
}

fn hotbar_label(stack: Option<ItemStack>) -> String {
    stack
        .map(|stack| {
            let name = stack.definition().name;
            let short = name.split('_').next().unwrap_or(name);
            if stack.count() > 1 {
                format!("{}\n{}", &short[..short.len().min(5)], stack.count())
            } else {
                short[..short.len().min(5)].to_string()
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
    mut labels: Query<(&HotbarItem, &mut Text, &mut Node)>,
    icons: Res<BlockIcons>,
) {
    let Ok(hotbar) = hotbar.single() else {
        return;
    };
    if !hotbar.is_changed() && !icons.is_changed() {
        return;
    }
    for (item, mut text, mut node) in &mut labels {
        let stack = hotbar.slots[item.0];
        let has_icon = stack
            .and_then(|stack| icons.rect_for_stack(stack))
            .is_some();
        let (left, top) = hotbar_item_rect(item.0);
        node.left = px(if has_icon { left + 14.0 } else { left });
        node.top = px(if has_icon { top + 16.0 } else { top });
        node.width = px(if has_icon {
            18.0
        } else {
            ITEM_SIZE * HUD_SCALE
        });
        **text = visible_label(stack, &icons);
    }
}

fn update_hotbar_icons(
    hotbar: Query<&Hotbar, With<Player>>,
    icons: Res<BlockIcons>,
    mut images: Query<(&HotbarBlockIcon, &mut ImageNode, &mut Visibility)>,
) {
    let Ok(hotbar) = hotbar.single() else {
        return;
    };
    for (slot, mut image, mut visibility) in &mut images {
        if let Some(rect) = hotbar.slots[slot.0].and_then(|stack| icons.rect_for_stack(stack)) {
            image.rect = Some(rect);
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
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
