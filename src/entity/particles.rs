//! Gameplay particle requests. Simulation writes here; rendering drains and
//! draws. Keeps boat splash and furnace-cart smoke out of `rendering/`.

use bevy::prelude::*;

/// Named Beta `spawnParticle` kinds this queue currently accepts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParticleKind {
    /// `splash` (`EntitySplashFX`).
    Splash,
    /// `largesmoke` (`EntitySmokeFX` at scale 2.5).
    LargeSmoke,
}

#[derive(Clone, Copy, Debug)]
pub struct ParticleEmit {
    pub kind: ParticleKind,
    pub position: Vec3,
    pub velocity: Vec3,
}

/// Pending world particles for the client to spawn.
#[derive(Resource, Default)]
pub struct ParticleEmits {
    pending: Vec<ParticleEmit>,
}

impl ParticleEmits {
    pub fn splash(&mut self, position: Vec3, velocity: Vec3) {
        self.pending.push(ParticleEmit {
            kind: ParticleKind::Splash,
            position,
            velocity,
        });
    }

    pub fn large_smoke(&mut self, position: Vec3) {
        self.pending.push(ParticleEmit {
            kind: ParticleKind::LargeSmoke,
            position,
            velocity: Vec3::ZERO,
        });
    }

    pub fn drain(&mut self) -> Vec<ParticleEmit> {
        std::mem::take(&mut self.pending)
    }
}
