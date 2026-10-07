//! Standing in a Nether portal: Beta's `EntityPlayerSP.onLivingUpdate`
//! portal counters, and the view distortion `EntityRenderer` draws from them.

use bevy::camera::visibility::VisibilitySystems;
use bevy::math::Affine3A;
use bevy::prelude::*;

use super::Player;
use super::PlayerCamera;
use crate::app::session::Travel;
use crate::app::session::WorldSession;
use crate::inventory::session::InventorySession;
use crate::rendering::sky::CelestialCamera;
use crate::rendering::sky::SkyCamera;
use crate::world::tick::WorldTick;

/// `EntityPlayer.timeUntilPortal` on spawning and after any arrival.
const SPAWN_COOLDOWN: u8 = 20;
/// The cooldown a portal holds a freshly arrived player at, and the one set
/// on leaving through it.
const PORTAL_COOLDOWN: u8 = 10;
/// Charge gained per tick in a portal: eighty ticks to travel.
const CHARGE_PER_TICK: f32 = 0.0125;
/// Charge lost per tick outside one.
const DECAY_PER_TICK: f32 = 0.05;

/// `EntityPlayer`'s `inPortal`, `timeInPortal`, `prevTimeInPortal` and
/// `timeUntilPortal`. Like Beta, none of it is saved.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PortalTravel {
    /// A portal block touched the player since the last portal tick.
    in_portal: bool,
    /// How far the trip has charged, `0..=1`.
    time: f32,
    previous_time: f32,
    /// Ticks before a portal can start charging. Standing in one holds it up.
    cooldown: u8,
}

impl Default for PortalTravel {
    fn default() -> Self {
        Self {
            in_portal: false,
            time: 0.0,
            previous_time: 0.0,
            cooldown: SPAWN_COOLDOWN,
        }
    }
}

impl PortalTravel {
    /// `EntityPlayer.setInPortal`, called by a portal block the player's box
    /// overlaps.
    pub fn set_in_portal(&mut self) {
        if self.cooldown > 0 {
            self.cooldown = PORTAL_COOLDOWN;
        } else {
            self.in_portal = true;
        }
    }

    /// One tick of `onLivingUpdate`'s portal block. Returns true on the tick
    /// the charge completes and the player should travel.
    pub fn tick(&mut self) -> bool {
        self.previous_time = self.time;
        let mut travel = false;
        if self.in_portal {
            self.time += CHARGE_PER_TICK;
            if self.time >= 1.0 {
                self.time = 1.0;
                self.cooldown = PORTAL_COOLDOWN;
                travel = true;
            }
            self.in_portal = false;
        } else {
            self.time = (self.time - DECAY_PER_TICK).max(0.0);
        }
        self.cooldown = self.cooldown.saturating_sub(1);
        travel
    }

    /// Whether a portal is charging, which closes any open screen.
    pub fn charging(&self) -> bool {
        self.in_portal
    }

    /// The charge to draw `partial` ticks past the last one.
    pub fn strength(&self, partial: f32) -> f32 {
        self.previous_time + (self.time - self.previous_time) * partial
    }

    pub fn cooldown(&self) -> u8 {
        self.cooldown
    }
}

/// `GuiIngame.renderPortalOverlay`'s opacity for a charge below one.
pub fn overlay_alpha(strength: f32) -> f32 {
    if strength >= 1.0 {
        return strength;
    }
    let squared = strength * strength;
    squared * squared * 0.8 + 0.2
}

/// The matrix `EntityRenderer` multiplies into the view while the player is
/// in a portal: a squeeze along an axis that turns about `(0, 1, 1)` twenty
/// degrees a tick. `ticks` is `rendererUpdateCount` plus the partial tick.
pub fn nausea_view(strength: f32, ticks: f32) -> Mat4 {
    if strength <= 0.0 {
        return Mat4::IDENTITY;
    }
    let squeeze = 5.0 / (strength * strength + 5.0) - strength * 0.04;
    let squeeze = squeeze * squeeze;
    let turn = Quat::from_axis_angle(
        Vec3::new(0.0, 1.0, 1.0).normalize(),
        (ticks * 20.0).to_radians(),
    );
    Mat4::from_quat(turn)
        * Mat4::from_scale(Vec3::new(1.0 / squeeze, 1.0, 1.0))
        * Mat4::from_quat(turn.inverse())
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        tick_portal_travel
            .after(crate::physics::PhysicsSet::Integrate)
            .run_if(in_state(crate::app::state::AppScreen::Playing)),
    )
    .add_systems(
        PostUpdate,
        distort_view
            .after(TransformSystems::Propagate)
            .before(VisibilitySystems::UpdateFrusta),
    );
}

fn tick_portal_travel(
    tick: Res<WorldTick>,
    mut players: Query<&mut PortalTravel, With<Player>>,
    mut session: Option<ResMut<WorldSession>>,
    mut inventory: Option<ResMut<InventorySession>>,
) {
    let ticks = tick.ticks_this_frame();
    if ticks == 0 {
        return;
    }
    for mut portal in &mut players {
        // Physics reports contact once for the frame's ticks; spend it on
        // each of them, as the player has not moved between.
        let touching = portal.in_portal;
        for _ in 0..ticks {
            portal.in_portal = touching;
            if touching
                && let Some(inventory) = inventory.as_deref_mut()
                && inventory.open
            {
                // `mc.displayGuiScreen(null)`.
                inventory.close_requested = true;
            }
            if portal.tick()
                && let Some(session) = session.as_deref_mut()
            {
                session.request_travel(Travel::Portal);
            }
        }
    }
}

/// Apply [`nausea_view`] to the cameras that draw the world and sky. The view
/// is the inverse of a camera's global transform, and the distortion is a
/// shear a local `Transform` cannot hold, so it goes onto the propagated
/// transform directly. The arm is drawn undistorted, as in Beta.
fn distort_view(
    tick: Option<Res<WorldTick>>,
    players: Query<&PortalTravel, With<Player>>,
    mut cameras: Query<
        &mut GlobalTransform,
        Or<(With<PlayerCamera>, With<SkyCamera>, With<CelestialCamera>)>,
    >,
) {
    let (Some(tick), Ok(portal)) = (tick, players.single()) else {
        return;
    };
    let strength = portal.strength(tick.partial());
    if strength <= 0.0 {
        return;
    }
    let ticks = (tick.world_time() % 3600) as f32 + tick.partial();
    let undo = Affine3A::from_mat4(nausea_view(strength, ticks).inverse());
    for mut camera in &mut cameras {
        *camera = GlobalTransform::from(camera.affine() * undo);
    }
}
