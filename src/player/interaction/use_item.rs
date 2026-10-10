//! `Item.onItemRightClick` for the items that launch something: the bow,
//! snowballs, eggs, and the fishing rod.
//!
//! `interact_blocks` decides what a right-click reached and reports the ones
//! these items answer as an [`ItemUse`]. Beta 1.7.3's bow shoots the moment
//! it is used. With the Bow Charging feature it is drawn while the button is
//! held and shoots on release, as Beta 1.8's does; that half is written from
//! memory of 1.8, which the reference source does not include.

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::entity::Velocity;
use crate::entity::fishing::Bobber;
use crate::entity::fishing::Fishing;
use crate::entity::fishing::cast_bobber;
use crate::entity::fishing::reel_in;
use crate::entity::mobs::Mob;
use crate::entity::projectiles::ArrowDamage;
use crate::entity::projectiles::spawn_player_arrow;
use crate::entity::thrown::ThrownKind;
use crate::entity::thrown::throw_from_player;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::session::InventorySession;
use crate::item::Item;
use crate::player::LocalPlayer;
use crate::player::Player;
use crate::player::sleep::PlayerSleep;
use crate::random::ItemRng;
use crate::random::JavaRandom;
use crate::world::tick::WorldTick;
use std::collections::HashMap;

/// `ItemBow`'s speed in Beta 1.7.3, in blocks per tick.
const BOW_SPEED: f32 = 1.5;
/// A charged bow shoots at up to twice that.
const FULL_DRAW_SPEED: f32 = 3.0;
/// Movement input is cut to this while the bow is drawn.
pub(crate) const DRAW_MOVEMENT_SCALE: f32 = 0.2;

/// The held item was used: a right-click that no block or screen took.
#[derive(Message, Clone, Copy, Debug)]
pub struct ItemUse {
    pub player: Entity,
    /// The player's eyes.
    pub eye: Vec3,
    /// Unit view direction.
    pub look: Vec3,
}

/// The Bow Charging feature: the player is drawing the bow.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BowDraw {
    /// World ticks the string has been held back.
    pub ticks: u32,
}

/// Whether using `item` launches something, so a click with it is reported
/// as an [`ItemUse`] rather than tried as a placement.
pub fn launches(item: Item) -> bool {
    matches!(
        item,
        Item::Bow | Item::Snowball | Item::Egg | Item::FishingRod
    )
}

/// How far the bow is drawn after `ticks`, from 0 to 1: Beta 1.8's
/// `(f * f + f * 2) / 3` for `f` in seconds.
pub fn draw_power(ticks: f32) -> f32 {
    let seconds = ticks / 20.0;
    ((seconds * seconds + seconds * 2.0) / 3.0).min(1.0)
}

/// `Entity.rand` for what the player launches.
pub(crate) struct LaunchRandom(JavaRandom);

impl Default for LaunchRandom {
    fn default() -> Self {
        Self(JavaRandom::new(0x4c41_554e))
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn use_items(
    mut commands: Commands,
    mut uses: MessageReader<ItemUse>,
    settings: Option<Res<GameSettings>>,
    mut player: Query<
        (
            Entity,
            &mut Hotbar,
            &mut Inventory,
            Option<&Fishing>,
            Option<&BowDraw>,
        ),
        With<Player>,
    >,
    bobbers: Query<(&Bobber, &Transform), Without<Player>>,
    mut mobs: Query<&mut Velocity, With<Mob>>,
    mut rng: Local<LaunchRandom>,
    mut item_rng: Local<ItemRng>,
    // Each player's line and draw as this frame's uses leave them: the
    // components only change once the commands are applied.
    mut hands: Local<HashMap<Entity, (Option<Entity>, bool)>>,
) {
    hands.clear();
    for (entity, _, _, fishing, draw) in &player {
        // The bobber is gone without the player's doing: a change of
        // dimension.
        let mut fishing = fishing.map(|fishing| fishing.0);
        if fishing.is_some_and(|bobber| !bobbers.contains(bobber)) {
            commands.entity(entity).remove::<Fishing>();
            fishing = None;
        }
        hands.insert(entity, (fishing, draw.is_some()));
    }
    let charging = settings.is_some_and(|settings| settings.bow_charging);
    for used in uses.read() {
        let Ok((entity, mut hotbar, mut inventory, ..)) = player.get_mut(used.player) else {
            continue;
        };
        let Some((fishing, drawing)) = hands.get_mut(&entity) else {
            continue;
        };
        let Some(item) = hotbar.selected_stack().map(|stack| stack.item()) else {
            continue;
        };
        match item {
            Item::Bow if charging => {
                // Beta 1.8 `ItemBow.onItemRightClick`: only with an arrow.
                if !*drawing && inventory.holds(&hotbar, Item::Arrow) {
                    commands.entity(entity).insert(BowDraw::default());
                    *drawing = true;
                }
            }
            Item::Bow => {
                if inventory.consume(&mut hotbar, Item::Arrow) {
                    spawn_player_arrow(
                        &mut commands,
                        entity,
                        used.eye,
                        used.look,
                        BOW_SPEED,
                        ArrowDamage::Flat(4),
                        &mut rng.0,
                    );
                }
            }
            Item::Snowball | Item::Egg => {
                if let Some(kind) = ThrownKind::from_item(item)
                    && hotbar.take_selected(1).is_some()
                {
                    throw_from_player(&mut commands, kind, entity, used.eye, used.look, &mut rng.0);
                }
            }
            Item::FishingRod => {
                if let Some(bobber) = fishing.take()
                    && let Ok((state, at)) = bobbers.get(bobber)
                {
                    let damage = reel_in(
                        &mut commands,
                        &mut item_rng,
                        bobber,
                        state,
                        at.translation,
                        used.eye,
                        &mut mobs,
                    );
                    if damage > 0 {
                        hotbar.damage_selected(damage);
                    }
                } else {
                    *fishing = Some(cast_bobber(
                        &mut commands,
                        entity,
                        used.eye,
                        used.look,
                        &mut rng.0,
                    ));
                }
            }
            _ => {}
        }
    }
}

/// The Bow Charging feature: count the draw while the button is held and
/// shoot when it is let go, as Beta 1.8's `onPlayerStoppedUsing` does. A draw
/// under a tenth of full strength shoots nothing and costs no arrow.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_bow(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mouse: Res<ButtonInput<MouseButton>>,
    inventory_screen: Option<Res<InventorySession>>,
    mut player: Query<
        (
            Entity,
            &Transform,
            &mut Hotbar,
            &mut Inventory,
            &mut BowDraw,
            Option<&PlayerSleep>,
        ),
        With<LocalPlayer>,
    >,
    camera: Query<&Transform, (With<crate::player::PlayerCamera>, Without<LocalPlayer>)>,
    mut rng: Local<LaunchRandom>,
) {
    let Ok((entity, transform, mut hotbar, mut inventory, mut draw, sleep)) = player.single_mut()
    else {
        return;
    };
    let holding_bow = hotbar
        .selected_stack()
        .is_some_and(|stack| stack.item() == Item::Bow);
    // Switching slot, opening a screen, or lying down lets the string go
    // slack without a shot.
    if !holding_bow
        || inventory_screen.is_some_and(|screen| screen.open)
        || sleep.is_some_and(|sleep| sleep.sleeping)
    {
        commands.entity(entity).remove::<BowDraw>();
        return;
    }
    if mouse.pressed(MouseButton::Right) {
        draw.ticks = draw.ticks.saturating_add(tick.ticks_this_frame());
        return;
    }
    commands.entity(entity).remove::<BowDraw>();
    let power = draw_power(draw.ticks as f32);
    if power < 0.1 || !inventory.consume(&mut hotbar, Item::Arrow) {
        return;
    }
    let view = transform.rotation
        * camera
            .single()
            .map_or(Quat::IDENTITY, |camera| camera.rotation);
    spawn_player_arrow(
        &mut commands,
        entity,
        transform.translation,
        view * Vec3::NEG_Z,
        power * FULL_DRAW_SPEED,
        ArrowDamage::Speed {
            critical: power >= 1.0,
        },
        &mut rng.0,
    );
}

pub(crate) fn plugin(app: &mut App) {
    app.add_message::<ItemUse>();
}
