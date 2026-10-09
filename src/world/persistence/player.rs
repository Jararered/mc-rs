//! The saved player: position, health, hazards, inventory, and game mode.

use super::FORMAT_VERSION;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::Item;
use crate::item::ItemStack;
use crate::world::dimension::Dimension;
use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

/// Player pose and inventory stored as `player.json` in a world folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredPlayer {
    pub format_version: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    #[serde(default = "full_player_health")]
    pub health: u8,
    /// `PlayerSurvival::air`.
    #[serde(default = "full_player_air")]
    pub air: i16,
    /// `PlayerSurvival::fire`.
    #[serde(default = "resting_player_fire")]
    pub fire: i16,
    #[serde(default)]
    pub fall_distance: f32,
    #[serde(default)]
    pub hotbar: Vec<Option<StoredStack>>,
    #[serde(default)]
    pub selected: usize,
    #[serde(default)]
    pub main: Vec<Option<StoredStack>>,
    #[serde(default)]
    pub crafting: Vec<Option<StoredStack>>,
    #[serde(default)]
    pub armor: Vec<Option<StoredStack>>,
    #[serde(default)]
    pub carried: Option<StoredStack>,
    #[serde(default)]
    pub flying: bool,
    #[serde(default)]
    pub fly_speed: f32,
    #[serde(default)]
    pub game_mode: crate::player::GameMode,
    /// Which dimension the position is in. Beta's `Dimension` tag.
    #[serde(default)]
    pub dimension: Dimension,
    /// The bed the player respawns at: `PlayerSleep::spawn`, Beta's
    /// `SpawnX`, `SpawnY` and `SpawnZ`.
    #[serde(default)]
    pub spawn: Option<[i32; 3]>,
}

pub(super) const fn full_player_health() -> u8 {
    crate::player::MAX_PLAYER_HEALTH
}

pub(super) const fn full_player_air() -> i16 {
    crate::entity::creature::MAX_AIR
}

/// Where Beta's `fire` rests for a player who is not burning.
pub(super) const fn resting_player_fire() -> i16 {
    -20
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredStack {
    pub id: u16,
    pub count: u8,
    pub data: u16,
}

impl StoredStack {
    pub(super) fn from_stack(stack: ItemStack) -> Self {
        Self {
            id: stack.item().as_u16(),
            count: stack.count(),
            data: stack.data(),
        }
    }
    pub(super) fn into_stack(self) -> Option<ItemStack> {
        let id = Item::from_u16(self.id)?;
        ItemStack::with_data(id, self.count, self.data).ok()
    }
}

impl StoredPlayer {
    pub fn from_transform(transform: &Transform) -> Self {
        let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
        Self {
            format_version: FORMAT_VERSION,
            x: transform.translation.x,
            y: transform.translation.y,
            z: transform.translation.z,
            yaw,
            pitch,
            health: full_player_health(),
            air: full_player_air(),
            fire: resting_player_fire(),
            fall_distance: 0.0,
            hotbar: Vec::new(),
            selected: 0,
            main: Vec::new(),
            crafting: Vec::new(),
            armor: Vec::new(),
            carried: None,
            flying: false,
            fly_speed: 1.0,
            game_mode: crate::player::GameMode::Survival,
            dimension: Dimension::Overworld,
            spawn: None,
        }
    }

    pub fn with_dimension(mut self, dimension: Dimension) -> Self {
        self.dimension = dimension;
        self
    }

    pub fn with_flying(mut self, flying: bool, fly_speed: f32) -> Self {
        self.flying = flying;
        self.fly_speed = fly_speed;
        self
    }

    pub fn with_game_mode(mut self, game_mode: crate::player::GameMode) -> Self {
        self.game_mode = game_mode;
        self
    }

    pub fn with_health(mut self, health: u8) -> Self {
        self.health = health;
        self
    }

    pub fn with_sleep(mut self, sleep: Option<&crate::player::sleep::PlayerSleep>) -> Self {
        self.spawn = sleep
            .and_then(|sleep| sleep.spawn)
            .map(|spawn| spawn.to_array());
        self
    }

    pub fn with_survival(mut self, survival: Option<&crate::player::PlayerSurvival>) -> Self {
        if let Some(survival) = survival {
            self.air = survival.air;
            self.fire = survival.fire;
            self.fall_distance = survival.fall_distance;
        }
        self
    }

    pub fn with_inventory(mut self, hotbar: &Hotbar, inventory: &Inventory) -> Self {
        self.hotbar = hotbar
            .slots
            .iter()
            .map(|slot| slot.map(StoredStack::from_stack))
            .collect();
        self.selected = hotbar.selected;
        self.main = inventory
            .main
            .iter()
            .map(|slot| slot.map(StoredStack::from_stack))
            .collect();
        self.crafting = inventory
            .crafting
            .iter()
            .map(|slot| slot.map(StoredStack::from_stack))
            .collect();
        self.armor = inventory
            .armor
            .iter()
            .map(|slot| slot.map(StoredStack::from_stack))
            .collect();
        self.carried = inventory.carried.map(StoredStack::from_stack);
        self
    }

    pub fn to_inventory(&self) -> (Hotbar, Inventory) {
        let mut hotbar = Hotbar::default();
        let mut inventory = Inventory::default();
        for (target, saved) in hotbar.slots.iter_mut().zip(&self.hotbar) {
            *target = saved.and_then(StoredStack::into_stack);
        }
        hotbar.select(self.selected);
        for (target, saved) in inventory.main.iter_mut().zip(&self.main) {
            *target = saved.and_then(StoredStack::into_stack);
        }
        for (target, saved) in inventory.crafting.iter_mut().zip(&self.crafting) {
            *target = saved.and_then(StoredStack::into_stack);
        }
        for (target, saved) in inventory.armor.iter_mut().zip(&self.armor) {
            *target = saved.and_then(StoredStack::into_stack);
        }
        inventory.carried = self.carried.and_then(StoredStack::into_stack);
        (hotbar, inventory)
    }

    pub fn to_transform(&self) -> Transform {
        Transform {
            translation: Vec3::new(self.x, self.y, self.z),
            rotation: Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0),
            ..default()
        }
    }
}
