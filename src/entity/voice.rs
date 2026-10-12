//! Mob voices: Beta's `getLivingSound`, `getHurtSound`, `getDeathSound`,
//! `getSoundVolume` and `getTalkInterval`, read from `data/sounds.ron`.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::entity::mobs::MobType;

/// `EntityLiving.getHurtSound` and `getDeathSound` where nothing overrides
/// them, which is the player.
pub const PLAYER_HURT: &str = "random.hurt";

/// What one kind of mob sounds like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    /// The idle sound, made at random between [`Voice::talk_interval`]s.
    pub living: Option<&'static str>,
    pub hurt: Option<&'static str>,
    pub death: Option<&'static str>,
    pub volume: f32,
    /// `livingSoundTime` starts this many ticks below zero after each call.
    pub talk_interval: i32,
}

/// One row of `data/sounds.ron`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    living: Option<String>,
    hurt: Option<String>,
    death: Option<String>,
    volume: Option<f32>,
    talk_interval: Option<i32>,
}

impl Row {
    fn build(self) -> Voice {
        // The table lives as long as the program, so its names may too.
        let name = |name: Option<String>| name.map(|name| &*name.leak());
        Voice {
            living: name(self.living),
            hurt: name(self.hurt),
            death: name(self.death),
            volume: self.volume.unwrap_or(1.0),
            talk_interval: self.talk_interval.unwrap_or(80),
        }
    }
}

static VOICES: LazyLock<HashMap<MobType, Voice>> = LazyLock::new(|| {
    let rows: HashMap<MobType, Row> = ron::from_str(include_str!("../../data/sounds.ron"))
        .unwrap_or_else(|error| panic!("data/sounds.ron: {error}"));
    for kind in MobType::ALL {
        assert!(
            rows.contains_key(&kind),
            "data/sounds.ron has a row for every mob; {kind:?} is missing"
        );
    }
    rows.into_iter()
        .map(|(kind, row)| (kind, row.build()))
        .collect()
});

pub fn voice(kind: MobType) -> &'static Voice {
    &VOICES[&kind]
}
