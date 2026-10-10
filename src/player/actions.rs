//! What a player asks of the world, and what the world tells a player back:
//! the messages that become packets once there is a wire.
//!
//! They follow Beta's protocol in shape. A client decides what its input
//! meant (which block the crosshair is on, when a dig has taken long enough)
//! and the world applies the result to the player named in the message, so
//! the same code serves this client's player and anyone else's.

use bevy::prelude::*;

use crate::physics::BlockHit;
use crate::world::chunk::ChestGroup;

/// An [`Action`] and the player performing it.
#[derive(Message, Clone, Copy, Debug)]
pub struct PlayerAction {
    pub player: Entity,
    pub action: Action,
}

/// Something a player does with their hands.
#[derive(Clone, Copy, Debug)]
pub enum Action {
    /// `Packet14BlockDig` status 4: drop one of the held item.
    DropItem,
    /// `Packet7UseEntity`: a click on what the crosshair rests on.
    UseEntity {
        target: Pointed,
        /// The left button: attack. Otherwise `interact`.
        attack: bool,
        /// Unit view direction.
        look: Vec3,
    },
    /// `Packet14BlockDig` status 0: the player starts digging `hit`
    /// (`Block.onBlockClicked`).
    StartDig { hit: BlockHit },
    /// `Packet14BlockDig` status 2: the dig is done and the block goes.
    Break { hit: BlockHit },
    /// `Packet15Place`: the use button, on the block in view if there is one.
    Use {
        hit: Option<BlockHit>,
        /// Where the view ray starts.
        origin: Vec3,
        /// Unit view direction.
        look: Vec3,
        /// A fresh press rather than the held-button repeat. Only a press
        /// eats food or opens a container.
        click: bool,
    },
}

/// What the crosshair rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointed {
    Mob(Entity),
    Fireball(Entity),
    Minecart(Entity),
    Boat(Entity),
}

/// `Packet100OpenWindow`: the world opened a container for `player`.
#[derive(Message, Clone, Copy, Debug)]
pub struct WindowOpen {
    pub player: Entity,
    pub window: Window,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    Workbench {
        position: IVec3,
    },
    Furnace {
        position: IVec3,
    },
    Chest {
        position: IVec3,
        group: ChestGroup,
    },
    /// A chest minecart, which was in `cell` when it was opened.
    CartChest {
        cart: Entity,
        cell: IVec3,
    },
}

/// Applies [`PlayerAction`]s. It needs no window, camera or input, so a
/// headless world can add it alone.
pub struct PlayerActionsPlugin;

/// The systems that apply [`PlayerAction`]s. Whatever writes them runs before.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerActionSet;

impl Plugin for PlayerActionsPlugin {
    fn build(&self, app: &mut App) {
        super::interaction::use_item::plugin(app);
        app.add_message::<PlayerAction>()
            .add_message::<WindowOpen>()
            .add_message::<super::sleep::BedUse>()
            .init_resource::<crate::inventory::session::ActiveWorkbench>()
            .add_systems(
                Update,
                (
                    super::interaction::editing::apply_player_actions,
                    super::interaction::use_item::use_items,
                )
                    .chain()
                    .in_set(PlayerActionSet)
                    .run_if(crate::world::tick::playing),
            );
    }
}
