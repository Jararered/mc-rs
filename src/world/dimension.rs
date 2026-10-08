//! The dimension being played, and the rules Beta's `WorldProvider` gives it.
//!
//! One save holds both dimensions. Only one is loaded at a time, as in Beta's
//! client: chunks, light, block ticks and entities all belong to
//! [`ActiveDimension`], and travelling through a portal unloads one and
//! streams the other.

use bevy::ecs::system::SystemParam;
use bevy::prelude::Res;
use bevy::prelude::Resource;
use serde::Deserialize;
use serde::Serialize;

use super::environment;
use super::tick::WorldTick;
use super::weather::WorldWeather;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dimension {
    #[default]
    Overworld,
    /// `WorldProviderHell`.
    Nether,
}

impl Dimension {
    /// Beta's `worldType`, stored as the player's `Dimension` tag.
    pub const fn id(self) -> i32 {
        match self {
            Self::Overworld => 0,
            Self::Nether => -1,
        }
    }

    /// `WorldProvider.getProviderForDimension`: anything but -1 is the surface.
    pub const fn from_id(id: i32) -> Self {
        if id == -1 {
            Self::Nether
        } else {
            Self::Overworld
        }
    }

    /// The dimension a portal leads to.
    pub const fn other(self) -> Self {
        match self {
            Self::Overworld => Self::Nether,
            Self::Nether => Self::Overworld,
        }
    }

    /// `!hasNoSky`: whether sunlight exists at all.
    pub const fn has_sky(self) -> bool {
        matches!(self, Self::Overworld)
    }

    /// `isHellWorld`: lava flows as far as water, and water boils away.
    pub const fn is_hell(self) -> bool {
        matches!(self, Self::Nether)
    }

    /// Rain, thunder and lightning only advance under a sky.
    pub const fn has_weather(self) -> bool {
        self.has_sky()
    }

    /// `canRespawnHere`.
    pub const fn can_respawn(self) -> bool {
        matches!(self, Self::Overworld)
    }

    /// The floor of `generateLightBrightnessTable`: how bright light level 0 is.
    pub const fn ambient_light(self) -> f32 {
        match self {
            Self::Overworld => 0.05,
            Self::Nether => 0.1,
        }
    }

    /// `calculateCelestialAngle`. The Nether holds it at 0.5.
    pub fn celestial_angle(self, world_time: u64, partial: f32) -> f32 {
        match self {
            Self::Overworld => environment::celestial_angle(world_time, partial),
            Self::Nether => 0.5,
        }
    }

    /// `World.calculateSkylightSubtracted` without weather. With no sky the
    /// whole sky channel is subtracted, so light estimated from a bare column
    /// (which would call an uncovered cell sunlit) comes out as block light
    /// alone, as `hasNoSky` leaves it.
    pub fn skylight_subtracted(self, world_time: u64, partial: f32) -> u8 {
        match self {
            Self::Overworld => {
                environment::skylight_subtracted(self.celestial_angle(world_time, partial))
            }
            Self::Nether => 15,
        }
    }

    /// `World.calculateSkylightSubtracted` with `rain` and weighted `thunder`
    /// strengths. Without a sky there is nothing for weather to dim.
    pub fn skylight_subtracted_in_weather(
        self,
        world_time: u64,
        partial: f32,
        rain: f32,
        thunder: f32,
    ) -> u8 {
        match self {
            Self::Overworld => environment::skylight_subtracted_in_weather(
                self.celestial_angle(world_time, partial),
                rain,
                thunder,
            ),
            Self::Nether => 15,
        }
    }

    /// Where the same horizontal position lies in `target`: one Nether block
    /// spans eight in the Overworld.
    pub fn scale_position_to(self, target: Self, x: f64, z: f64) -> (f64, f64) {
        match (self, target) {
            (Self::Overworld, Self::Nether) => (x / 8.0, z / 8.0),
            (Self::Nether, Self::Overworld) => (x * 8.0, z * 8.0),
            _ => (x, z),
        }
    }

    /// The folder inside a world that holds this dimension's chunks, as
    /// `SaveOldDir.getChunkLoader` chooses it.
    pub const fn folder(self) -> Option<&'static str> {
        match self {
            Self::Overworld => None,
            Self::Nether => Some("DIM-1"),
        }
    }
}

/// The dimension whose chunks are loaded. Reset when a world unloads.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActiveDimension(pub Dimension);

/// The sky, light and weather rules in force right now: the shared clock and
/// weather as the active dimension sees them. Systems that light or shade
/// something read this rather than calling the Overworld's daylight functions.
#[derive(SystemParam)]
pub struct Environment<'w> {
    tick: Option<Res<'w, WorldTick>>,
    dimension: Option<Res<'w, ActiveDimension>>,
    weather: Option<Res<'w, WorldWeather>>,
}

impl Environment<'_> {
    pub fn dimension(&self) -> Dimension {
        self.dimension
            .as_ref()
            .map_or_else(Dimension::default, |d| d.0)
    }

    /// Rain as this dimension has it: never, without a sky.
    pub fn is_raining(&self) -> bool {
        self.dimension().has_weather() && self.weather.as_ref().is_some_and(|w| w.is_raining())
    }

    /// Rain strength and weighted thunder strength as this dimension has
    /// them: none without a sky.
    pub fn weather_strength(&self) -> (f32, f32) {
        if !self.dimension().has_weather() {
            return (0.0, 0.0);
        }
        self.weather
            .as_ref()
            .map_or((0.0, 0.0), |w| (w.rain_strength, w.weighted_thunder()))
    }

    /// `World.skylightSubtracted` at `partial` ticks past the current one:
    /// time of day plus weather.
    pub fn skylight_subtracted(&self, partial: f32) -> u8 {
        let time = self.tick.as_ref().map_or(0, |tick| tick.world_time());
        let (rain, thunder) = self.weather_strength();
        self.dimension()
            .skylight_subtracted_in_weather(time, partial, rain, thunder)
    }

    pub fn celestial_angle(&self, partial: f32) -> f32 {
        let time = self.tick.as_ref().map_or(0, |tick| tick.world_time());
        self.dimension().celestial_angle(time, partial)
    }

    pub fn ambient_light(&self) -> f32 {
        self.dimension().ambient_light()
    }
}
