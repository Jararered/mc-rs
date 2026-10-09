//! What the world does to the player each tick: Beta's `Entity.onEntityUpdate`
//! and `EntityLiving.onEntityUpdate` hazards (burning, lava, the void,
//! suffocation, drowning) and the hazards at the tail of `Entity.moveEntity`
//! (fall damage, cactus, fire contact), then death and respawning. Every hit
//! goes through [`hurt_player`], so armor and the invulnerability window apply
//! to all of them as they do in Beta.

use bevy::prelude::*;

use super::GameMode;
use super::MAX_PLAYER_HEALTH;
use super::Player;
use super::PlayerHealth;
use super::PlayerInterpolation;
use super::default_spawn_transform;
use super::sleep::BED_MISSING_MESSAGE;
use super::sleep::PlayerSleep;
use super::sleep::bed_chunks;
use super::sleep::bed_respawn_feet;
use crate::app::session::Travel;
use crate::app::session::WorldSession;
use crate::app::state::AppScreen;
use crate::chat::ChatHistory;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Flying;
use crate::entity::Velocity;
use crate::entity::combat::Hit;
use crate::entity::combat::PlayerCombat;
use crate::entity::combat::Victim;
use crate::entity::combat::hurt_player;
use crate::entity::combat::tick_player_combat;
use crate::entity::creature::MAX_AIR;
use crate::entity::creature::PLAYER_EYE_HEIGHT;
use crate::entity::drops::items::spawn_block_drop;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::PhysicsSet;
use crate::physics::burning_in;
use crate::physics::eye_in_water;
use crate::physics::inside_opaque_block;
use crate::physics::lava_contains;
use crate::physics::touches_cactus;
use crate::physics::water_movement;
use crate::random::ItemRng;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::difficulty::Difficulty;
use crate::world::dimension::Environment;
use crate::world::persistence::WorldPersistence;
use crate::world::tick::WorldTick;

/// `EntityPlayer.fireResistance`: `fire` rests this far below zero, so a fire
/// block takes a second to set the player alight.
const FIRE_RESISTANCE: i16 = 20;
/// `air` at which a submerged player takes a drowning hit.
const DROWNING_AIR: i16 = -20;
/// Ticks between dying and reappearing at the spawn point.
const RESPAWN_TICKS: u16 = 40;

/// Runs the player's hazards after physics. It works in an app without
/// [`AppScreen`], where it always runs.
pub struct SurvivalPlugin;

impl Plugin for SurvivalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                apply_game_mode.before(PhysicsSet::ApplyInput),
                tick_player_survival
                    .after(PhysicsSet::Integrate)
                    .before(tick_player_combat),
            )
                .run_if(|screen: Option<Res<State<AppScreen>>>| {
                    screen.is_none_or(|screen| *screen.get() == AppScreen::Playing)
                }),
        );
    }
}

/// The player's air supply, fire, and fall: Beta's `Entity.air`, `fire`, and
/// `fallDistance`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PlayerSurvival {
    /// Ticks of breath left. It runs past zero to -19; reaching -20 deals a
    /// drowning hit and puts it back to zero.
    pub air: i16,
    /// Above zero the player is burning for that many ticks. It rests at
    /// `-FIRE_RESISTANCE` and climbs toward zero while touching fire.
    pub fire: i16,
    /// Blocks fallen since the player last stood on something.
    pub fall_distance: f32,
    /// A fall that physics has finished and the hazard pass has yet to turn
    /// into damage.
    pub landed: f32,
    /// `isInsideOfMaterial(Material.water)` as of the last tick. The HUD
    /// shows the bubble row only while this holds.
    pub head_in_water: bool,
    death_ticks: u16,
}

impl Default for PlayerSurvival {
    fn default() -> Self {
        Self {
            air: MAX_AIR,
            fire: -FIRE_RESISTANCE,
            fall_distance: 0.0,
            landed: 0.0,
            head_in_water: false,
            death_ticks: 0,
        }
    }
}

/// How one HUD air bubble is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bubble {
    Empty,
    Popping,
    Full,
}

impl PlayerSurvival {
    /// The state a save holds.
    pub fn restored(air: i16, fire: i16, fall_distance: f32) -> Self {
        Self {
            air: air.clamp(DROWNING_AIR + 1, MAX_AIR),
            fire,
            fall_distance: fall_distance.max(0.0),
            ..Self::default()
        }
    }

    pub fn is_burning(&self) -> bool {
        self.fire > 0
    }

    /// `GuiIngame`'s air row: bubble `index` of ten, counted from the left.
    /// Nothing is drawn with the head above water.
    pub fn bubble(&self, index: usize) -> Bubble {
        if !self.head_in_water {
            return Bubble::Empty;
        }
        let air = f64::from(self.air);
        let full = ((air - 2.0) * 10.0 / f64::from(MAX_AIR)).ceil() as i32;
        let drawn = (air * 10.0 / f64::from(MAX_AIR)).ceil() as i32;
        let index = index as i32;
        if index < full {
            Bubble::Full
        } else if index < drawn {
            Bubble::Popping
        } else {
            Bubble::Empty
        }
    }

    /// `Entity.updateFallState` for a move that asked for `dy`: landing ends
    /// the fall, and any other downward move lengthens it.
    pub fn update_fall(&mut self, dy: f32, on_ground: bool) {
        if on_ground {
            if self.fall_distance > 0.0 {
                self.landed = self.landed.max(self.fall_distance);
                self.fall_distance = 0.0;
            }
        } else if dy < 0.0 {
            self.fall_distance -= dy;
        }
    }

    /// Forget a fall in progress, as flight does.
    pub fn clear_fall(&mut self) {
        if self.fall_distance != 0.0 || self.landed != 0.0 {
            self.fall_distance = 0.0;
            self.landed = 0.0;
        }
    }
}

/// Keeps flight and invulnerability in step with the player's [`GameMode`]:
/// survival never flies, a spectator always does, and only survival is hurt.
fn apply_game_mode(
    mut player: Query<(Entity, &GameMode, Has<Flying>, &mut PlayerCombat), With<Player>>,
    mut commands: Commands,
) {
    let Ok((entity, mode, flying, mut combat)) = player.single_mut() else {
        return;
    };
    match mode {
        GameMode::Survival if flying => {
            commands.entity(entity).remove::<Flying>();
        }
        GameMode::Spectator if !flying => {
            commands.entity(entity).insert(Flying);
        }
        _ => {}
    }
    if combat.invulnerable == mode.takes_damage() {
        combat.invulnerable = !mode.takes_damage();
    }
}

/// `World.canBlockBeRainedOn`, the rain half of `Entity.isWet`.
fn rained_on(chunks: &WorldChunks, cell: IVec3) -> bool {
    chunks
        .get(ChunkPosition::from_block(cell.x, cell.z))
        .is_some_and(|chunk| {
            let local_x = cell.x.rem_euclid(CHUNK_SIZE as i32) as usize;
            let local_z = cell.z.rem_euclid(CHUNK_SIZE as i32) as usize;
            cell.y >= i32::from(chunk.heightmap.get(local_x, local_z))
        })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn tick_player_survival(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    environment: Environment,
    mut player: Query<
        (
            Entity,
            &mut Transform,
            &mut PlayerHealth,
            &mut PlayerCombat,
            &mut PlayerSurvival,
            &mut Velocity,
            &mut CollisionState,
            &mut PlayerInterpolation,
            Option<&mut Hotbar>,
            Option<&mut Inventory>,
            &GameMode,
            &mut PlayerSleep,
        ),
        With<Player>,
    >,
    mut chat: Option<ResMut<ChatHistory>>,
    mut commands: Commands,
    mut rng: Local<ItemRng>,
    mut spare_armor: Local<[Option<ItemStack>; 4]>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut session: Option<ResMut<WorldSession>>,
) {
    let Ok((
        player_entity,
        mut transform,
        mut health,
        mut combat,
        mut survival,
        mut velocity,
        mut collision,
        mut interpolation,
        mut hotbar,
        mut inventory,
        mode,
        mut sleep,
    )) = player.single_mut()
    else {
        return;
    };
    let raining = environment.is_raining();
    // The HUD and inventory screens redraw on change, so hits write through
    // and the change is flagged once below.
    let health_before = health.current;
    let armor_before = inventory.as_deref().map(|inventory| inventory.armor);
    for _ in 0..tick.ticks_this_frame() {
        let health = health.bypass_change_detection();
        if health.current == 0 {
            if survival.death_ticks == 0 {
                let cell = (transform.translation - Vec3::Y * EntitySize::PLAYER.y_offset)
                    .floor()
                    .as_ivec3();
                let mut drop = |slot: &mut Option<ItemStack>| {
                    if let Some(stack) = slot.take() {
                        spawn_block_drop(&mut commands, &mut rng, cell, stack);
                    }
                };
                if let Some(hotbar) = hotbar.as_deref_mut() {
                    hotbar.slots.iter_mut().for_each(&mut drop);
                }
                if let Some(inventory) = inventory.as_deref_mut() {
                    let Inventory {
                        main,
                        crafting,
                        armor,
                        carried,
                    } = inventory;
                    main.iter_mut()
                        .chain(crafting.iter_mut())
                        .chain(armor.iter_mut())
                        .chain(std::iter::once(carried))
                        .for_each(&mut drop);
                }
                if let Some(persistence) = persistence.as_deref_mut() {
                    persistence.mark_dirty(ChunkPosition::from_block(cell.x, cell.z));
                }
                survival.death_ticks = RESPAWN_TICKS;
            } else {
                survival.death_ticks -= 1;
                if survival.death_ticks == 0 {
                    // `Minecraft.respawn`: where `canRespawnHere` is false
                    // the player leaves for the Overworld first, and is put
                    // at the spawn point once it has loaded.
                    // A bed whose chunks are not loaded is looked at the same
                    // way: the world is reloaded around it.
                    let here = environment.dimension().can_respawn();
                    let bed = sleep.spawn.filter(|_| here);
                    let bed_loaded =
                        bed.is_some_and(|bed| bed_chunks(bed).all(|chunk| chunks.contains(chunk)));
                    if (!here || (bed.is_some() && !bed_loaded))
                        && let Some(session) = session.as_deref_mut()
                    {
                        session.request_travel(Travel::Respawn);
                    } else {
                        *transform = default_spawn_transform(&chunks);
                        if let Some(bed) = bed.filter(|_| bed_loaded) {
                            match bed_respawn_feet(&chunks, bed) {
                                Some(feet) => {
                                    *transform = Transform::from_translation(
                                        feet + Vec3::Y * EntitySize::PLAYER.y_offset,
                                    )
                                    .looking_to(Vec3::Z, Vec3::Y);
                                }
                                None => {
                                    sleep.spawn = None;
                                    if let Some(chat) = chat.as_deref_mut() {
                                        chat.push(BED_MISSING_MESSAGE);
                                    }
                                }
                            }
                        }
                        interpolation.previous_position = transform.translation;
                    }
                    // The new body is not on the old one's cart.
                    crate::entity::mount::detach(&mut commands, player_entity);
                    sleep.sleeping = false;
                    sleep.timer = 0;
                    velocity.0 = Vec3::ZERO;
                    *collision = CollisionState::default();
                    *combat = PlayerCombat::default();
                    *survival = PlayerSurvival::default();
                    health.current = MAX_PLAYER_HEALTH;
                }
            }
            continue;
        }
        if !mode.takes_damage() {
            // Nothing burns, drowns, or bruises this player, and no fall
            // waits for a return to survival.
            *survival = PlayerSurvival::default();
            continue;
        }

        let position = transform.translation;
        let aabb = EntitySize::PLAYER.aabb(position);
        let eye = position + Vec3::Y * PLAYER_EYE_HEIGHT;
        let (bevy_yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
        let mut victim = Victim {
            health,
            combat: &mut combat,
            velocity: &mut velocity,
            armor: match inventory.as_mut() {
                Some(inventory) => &mut inventory.bypass_change_detection().armor,
                None => &mut spare_armor,
            },
            eye: position,
            yaw: (std::f32::consts::PI - bevy_yaw).to_degrees(),
        };
        // No hazard has an attacker, so difficulty never scales one.
        let mut hurt = |amount: i16| {
            hurt_player(
                &mut victim,
                Hit::environment(amount),
                Difficulty::Normal,
                &mut rng,
            );
        };
        let state = &mut *survival;

        // `Entity.onEntityUpdate`.
        let band = Aabb::new(
            aabb.min + Vec3::new(0.001, 0.401, 0.001),
            aabb.max - Vec3::new(0.001, 0.401, 0.001),
        );
        let (in_water, _) = water_movement(band, &chunks);
        if in_water {
            state.fire = 0;
        }
        if state.fire > 0 {
            if state.fire % 20 == 0 {
                hurt(1);
            }
            state.fire -= 1;
        }
        if lava_contains(aabb, &chunks) {
            hurt(4);
            state.fire = 600;
        }
        if position.y < -64.0 {
            hurt(4);
        }

        // `EntityLiving.onEntityUpdate`.
        // `EntityPlayer.isEntityInsideOpaqueBlock`: never while asleep.
        if !sleep.sleeping && inside_opaque_block(eye, EntitySize::PLAYER.width, &chunks) {
            hurt(1);
        }
        state.head_in_water = eye_in_water(eye, &chunks);
        if state.head_in_water {
            state.air -= 1;
            if state.air == DROWNING_AIR {
                state.air = 0;
                hurt(2);
            }
            state.fire = 0;
        } else {
            state.air = MAX_AIR;
        }

        // The tail of `Entity.moveEntity`.
        let fallen = std::mem::take(&mut state.landed);
        let damage = (fallen - 3.0).ceil() as i16;
        if damage > 0 {
            hurt(damage);
        }
        if touches_cactus(aabb, &chunks) {
            hurt(1);
        }
        let wet = in_water || (raining && rained_on(&chunks, position.floor().as_ivec3()));
        let inset = Aabb::new(aabb.min + Vec3::splat(0.001), aabb.max - Vec3::splat(0.001));
        if burning_in(inset, &chunks) {
            hurt(1);
            if !wet {
                state.fire += 1;
                if state.fire == 0 {
                    state.fire = 300;
                }
            }
        } else if state.fire <= 0 {
            state.fire = -FIRE_RESISTANCE;
        }
        if wet && state.fire > 0 {
            state.fire = -FIRE_RESISTANCE;
        }
    }
    if health.current != health_before {
        health.set_changed();
    }
    if let Some(inventory) = inventory.as_mut()
        && Some(inventory.armor) != armor_before
    {
        inventory.set_changed();
    }
}
