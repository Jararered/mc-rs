//! Named sprites in the original `particles.png` atlas. This describes art,
//! not particle simulation; callers can use the same sprite in world or UI.

use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::prelude::*;

pub const PARTICLE_ATLAS_GRID: u8 = 16;
pub const PARTICLE_TILE_PX: u8 = 8;

/// The loaded particle sheet. The handle is usable even when a local texture
/// pack does not provide `particles.png` yet.
#[derive(Resource, Clone)]
pub struct ParticleAtlas(pub Handle<Image>);

pub struct ParticleRegistryPlugin;

impl Plugin for ParticleRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_particle_atlas);
    }
}

fn load_particle_atlas(mut commands: Commands, asset_server: Res<AssetServer>) {
    let image = asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::nearest();
        })
        .load("particles.png");
    commands.insert_resource(ParticleAtlas(image));
}

/// A sprite in the 16×16 grid of 8×8 tiles. Variant names describe the
/// current art; effect behavior and animation timing belong to callers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ParticleSprite {
    Explosion(u8),
    WaterSplash(u8),
    AirBubble,
    FishingLure,
    Flame,
    Lava,
    MusicNote,
    HealthHeart,
    SoulSandStep(u8),
}

impl ParticleSprite {
    /// Every nonempty sprite in the local Beta sheet, in atlas order.
    pub const ALL: [Self; 22] = [
        Self::Explosion(0),
        Self::Explosion(1),
        Self::Explosion(2),
        Self::Explosion(3),
        Self::Explosion(4),
        Self::Explosion(5),
        Self::Explosion(6),
        Self::Explosion(7),
        Self::WaterSplash(0),
        Self::WaterSplash(1),
        Self::WaterSplash(2),
        Self::WaterSplash(3),
        Self::WaterSplash(4),
        Self::WaterSplash(5),
        Self::AirBubble,
        Self::FishingLure,
        Self::Flame,
        Self::Lava,
        Self::MusicNote,
        Self::HealthHeart,
        Self::SoulSandStep(0),
        Self::SoulSandStep(1),
    ];

    /// Atlas column and row. Returns `None` for a frame outside its group.
    pub const fn tile(self) -> Option<(u8, u8)> {
        match self {
            Self::Explosion(frame) if frame < 8 => Some((frame, 0)),
            // The splash row has empty columns 2 and 7; frame numbers skip them.
            Self::WaterSplash(frame) => match frame {
                0 | 1 => Some((frame, 1)),
                2..=5 => Some((frame + 1, 1)),
                _ => None,
            },
            Self::AirBubble => Some((0, 2)),
            Self::FishingLure => Some((1, 2)),
            Self::Flame => Some((0, 3)),
            Self::Lava => Some((1, 3)),
            Self::MusicNote => Some((0, 4)),
            Self::HealthHeart => Some((0, 5)),
            Self::SoulSandStep(frame) if frame < 2 => Some((frame, 6)),
            _ => None,
        }
    }

    /// Source rectangle in atlas pixels, suitable for `ImageNode::with_rect`.
    pub fn pixel_rect(self) -> Option<Rect> {
        let (x, y) = self.tile()?;
        let size = f32::from(PARTICLE_TILE_PX);
        let left = f32::from(x) * size;
        let top = f32::from(y) * size;
        Some(Rect::new(left, top, left + size, top + size))
    }

    /// Normalized UV bounds `(u_min, v_min, u_max, v_max)` for mesh vertices.
    pub fn uvs(self) -> Option<(f32, f32, f32, f32)> {
        let (x, y) = self.tile()?;
        let grid = f32::from(PARTICLE_ATLAS_GRID);
        Some((
            f32::from(x) / grid,
            f32::from(y) / grid,
            f32::from(x + 1) / grid,
            f32::from(y + 1) / grid,
        ))
    }
}
