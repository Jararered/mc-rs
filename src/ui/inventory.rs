use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::app::state::AppScreen;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::player::Player;

const SCALE: f32 = 2.0;

pub struct InventoryGuiPlugin;

impl Plugin for InventoryGuiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryScreen>()
            .add_systems(PreStartup, load_texture)
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
    terrain: Handle<Image>,
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
struct CarriedLabel;

fn load_texture(mut commands: Commands, assets: Res<AssetServer>) {
    let texture = assets
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::nearest()
        })
        .load("gui/inventory.png");
    let terrain = assets
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::nearest()
        })
        .load("terrain.png");
    commands.insert_resource(InventoryTexture {
        background: texture,
        terrain,
    });
}

fn toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut screen: ResMut<InventoryScreen>,
    texture: Res<InventoryTexture>,
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
        spawn(&mut commands, &texture.background, &texture.terrain);
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

fn spawn(commands: &mut Commands, texture: &Handle<Image>, terrain: &Handle<Image>) {
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
                        terrain,
                        Slot::Main(index),
                        8.0 + (index % 9) as f32 * 18.0,
                        84.0 + (index / 9) as f32 * 18.0,
                    );
                }
                for index in 0..9 {
                    slot(
                        panel,
                        terrain,
                        Slot::Hotbar(index),
                        8.0 + index as f32 * 18.0,
                        142.0,
                    );
                }
                for index in 0..4 {
                    slot(
                        panel,
                        terrain,
                        Slot::Craft(index),
                        88.0 + (index % 2) as f32 * 18.0,
                        26.0 + (index / 2) as f32 * 18.0,
                    );
                }
                for index in 0..4 {
                    slot(
                        panel,
                        terrain,
                        Slot::Armor(index),
                        8.0,
                        8.0 + index as f32 * 18.0,
                    );
                }
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
    terrain: &Handle<Image>,
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
                ImageNode::new(terrain.clone()),
                Node {
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
                    font_size: 11.0.into(),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextShadow::default(),
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
    mut labels: Query<(&SlotLabel, &mut Text)>,
    mut icons: Query<(&SlotIcon, &mut ImageNode, &mut Visibility)>,
    mut carried: Query<(&mut Text, &mut Node), (With<CarriedLabel>, Without<SlotLabel>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if !screen.open {
        return;
    }
    let Ok((hotbar, inventory)) = player.single() else {
        return;
    };
    for (label, mut text) in &mut labels {
        let stack = match label.0 {
            Slot::Hotbar(i) => hotbar.slots[i],
            Slot::Main(i) => inventory.main[i],
            Slot::Craft(i) => inventory.crafting[i],
            Slot::Armor(i) => inventory.armor[i],
        };
        **text = if stack.and_then(ItemStack::runtime_block).is_some() {
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
    for (icon, mut image, mut visibility) in &mut icons {
        let stack = match icon.0 {
            Slot::Hotbar(i) => hotbar.slots[i],
            Slot::Main(i) => inventory.main[i],
            Slot::Craft(i) => inventory.crafting[i],
            Slot::Armor(i) => inventory.armor[i],
        };
        if let Some(block) = stack.and_then(ItemStack::runtime_block) {
            let (x, y) = crate::world::textures::block_tile(block, 0, true);
            image.rect = Some(bevy::math::Rect::new(
                x as f32 * 16.0,
                y as f32 * 16.0,
                x as f32 * 16.0 + 16.0,
                y as f32 * 16.0 + 16.0,
            ));
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
    if let Ok((mut text, mut node)) = carried.single_mut() {
        **text = stack_text(inventory.carried);
        if let Ok(window) = windows.single() {
            if let Some(pos) = window.cursor_position() {
                node.left = px(pos.x - (window.width() - 176.0 * SCALE) / 2.0 + 8.0);
                node.top = px(pos.y - (window.height() - 166.0 * SCALE) / 2.0 + 8.0);
            }
        }
    }
}
