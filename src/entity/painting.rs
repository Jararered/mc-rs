//! `EntityPainting`, `EnumArt` and `ItemPainting`: a motive hung on a wall.
//!
//! The motives are `data/paintings.ron`. A painting is fixed to the wall
//! block it was hung on ([`Painting::tile`]) and faces one of four ways.
//! `Transform.translation` is Beta's `posX/posY/posZ`, the centre of its
//! box, which [`Painting::center`] derives again from the tile. Every 101
//! ticks it checks its wall, and falls off as a painting item when the wall
//! no longer holds it. Any hit, or an explosion, knocks it off too.
//!
//! Not copied: `getCollidingBoundingBoxes` also counts other entities' solid
//! boxes, such as a boat's, and this game gives entities none, so only
//! blocks in front of the wall stop a painting.

use std::sync::LazyLock;

use bevy::prelude::*;
use serde::Deserialize;

use crate::block::blocks::Block;
use crate::entity::drops::items::spawn_constructed_drop;
use crate::item::Item;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::BlockFace;
use crate::physics::collides;
use crate::random::ItemRng;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::edit::WorldEdit;
use crate::world::tick::WorldTick;

/// One row of `data/paintings.ron`: an `EnumArt` value.
#[derive(Debug, Deserialize)]
pub struct Motive {
    pub title: String,
    /// Width and height in pixels, 16 to a block.
    pub size: (u16, u16),
    /// The top-left corner on the 256×256 `art/kz.png`.
    pub offset: (u16, u16),
}

impl Motive {
    /// Width and height in blocks.
    pub fn blocks(&self) -> (i32, i32) {
        (i32::from(self.size.0 / 16), i32::from(self.size.1 / 16))
    }
}

static MOTIVES: LazyLock<Vec<Motive>> = LazyLock::new(|| {
    let motives: Vec<Motive> = ron::from_str(include_str!("../../data/paintings.ron"))
        .unwrap_or_else(|error| panic!("data/paintings.ron: {error}"));
    assert!(!motives.is_empty(), "data/paintings.ron lists no motive");
    for motive in &motives {
        let (width, height) = motive.size;
        assert!(
            width >= 16 && height >= 16 && width % 16 == 0 && height % 16 == 0,
            "data/paintings.ron: {} is not whole blocks",
            motive.title
        );
    }
    motives
});

/// Every motive, in `EnumArt` order.
pub fn motives() -> &'static [Motive] {
    &MOTIVES
}

/// A motive, by its place in [`motives`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Art(pub u8);

impl Art {
    pub fn all() -> impl Iterator<Item = Self> {
        (0..motives().len()).map(|index| Self(index as u8))
    }

    /// The motive titled `title`, or the first (Kebab) when none is, as
    /// `readEntityFromNBT` falls back.
    pub fn named(title: &str) -> Self {
        Self::find(title).unwrap_or_default()
    }

    pub fn find(title: &str) -> Option<Self> {
        Self::all().find(|art| art.title() == title)
    }

    pub fn motive(self) -> &'static Motive {
        &motives()[usize::from(self.0)]
    }

    pub fn title(self) -> &'static str {
        &self.motive().title
    }
}

/// How far a painting's centre stands out from its wall block's centre.
const WALL_OFFSET: f32 = 0.5625;
/// Ticks between checks of the wall: `tickCounter1++ == 100`.
const CHECK_TICKS: u8 = 100;

#[derive(Component, Clone, Debug, PartialEq)]
pub struct Painting {
    /// The wall block (`xPosition`, `yPosition`, `zPosition`).
    pub tile: IVec3,
    /// 0 faces north (−Z), 1 west, 2 south, 3 east.
    pub direction: u8,
    pub art: Art,
    /// `tickCounter1`. Not saved.
    pub ticks: u8,
}

impl Painting {
    pub fn new(tile: IVec3, direction: u8, art: Art) -> Self {
        Self {
            tile,
            direction: direction & 3,
            art,
            ticks: 0,
        }
    }

    /// `ItemPainting.onItemUse`: the way a painting hung on `face` looks.
    /// The top and bottom of a block take none.
    pub fn direction_for(face: BlockFace) -> Option<u8> {
        match face {
            BlockFace::Down | BlockFace::Up => None,
            BlockFace::North => Some(0),
            BlockFace::West => Some(1),
            BlockFace::South => Some(2),
            BlockFace::East => Some(3),
        }
    }

    /// `rotationYaw`, in degrees.
    pub fn yaw(&self) -> f32 {
        f32::from(self.direction) * 90.0
    }

    /// `setDirection`'s position: in front of the wall block, shifted so a
    /// motive two or four blocks across covers the blocks beside it.
    pub fn center(&self) -> Vec3 {
        let motive = self.art.motive();
        let mut center = self.tile.as_vec3() + Vec3::splat(0.5);
        match self.direction {
            0 => center.z -= WALL_OFFSET,
            1 => center.x -= WALL_OFFSET,
            2 => center.z += WALL_OFFSET,
            _ => center.x += WALL_OFFSET,
        }
        let across = art_size(motive.size.0);
        match self.direction {
            0 => center.x -= across,
            1 => center.z += across,
            2 => center.x += across,
            _ => center.z -= across,
        }
        center.y += art_size(motive.size.1);
        center
    }

    /// `setDirection`'s bounding box: half a pixel deep, inset slightly on
    /// every side.
    pub fn aabb(&self) -> Aabb {
        let motive = self.art.motive();
        let across = f32::from(motive.size.0) / 32.0;
        let thin = 0.5 / 32.0;
        let half = if self.direction % 2 == 0 {
            Vec3::new(across, f32::from(motive.size.1) / 32.0, thin)
        } else {
            Vec3::new(thin, f32::from(motive.size.1) / 32.0, across)
        };
        let inset = Vec3::splat(0.00625);
        let center = self.center();
        Aabb::new(center - half + inset, center + half - inset)
    }

    /// The wall blocks `onValidSurface` needs solid, left to right as seen
    /// from in front, bottom to top.
    pub fn wall(&self) -> impl Iterator<Item = IVec3> + use<> {
        let motive = self.art.motive();
        let (width, height) = motive.blocks();
        let center = self.center();
        let along = |position: f32| (position - f32::from(motive.size.0) / 32.0).floor() as i32;
        let bottom = (center.y - f32::from(motive.size.1) / 32.0).floor() as i32;
        let tile = self.tile;
        let sideways = self.direction % 2 == 1;
        let start = if sideways {
            along(center.z)
        } else {
            along(center.x)
        };
        (0..width).flat_map(move |across| {
            (0..height).map(move |up| {
                if sideways {
                    IVec3::new(tile.x, bottom + up, start + across)
                } else {
                    IVec3::new(start + across, bottom + up, tile.z)
                }
            })
        })
    }

    /// `onValidSurface`: nothing solid in front, a solid wall behind every
    /// block of the motive, and no other painting in the way. `others` are
    /// the other paintings' boxes.
    pub fn on_valid_surface(
        &self,
        chunks: &WorldChunks,
        others: impl IntoIterator<Item = Aabb>,
    ) -> bool {
        let aabb = self.aabb();
        if collides(chunks, aabb) {
            return false;
        }
        let solid = |cell: IVec3| {
            chunks
                .block_at(cell.x, cell.y, cell.z)
                .is_some_and(Block::is_solid_material)
        };
        if !self.wall().all(solid) {
            return false;
        }
        !others.into_iter().any(|other| other.intersects(aabb))
    }

    /// `ItemPainting.onItemUse` and the `EntityPainting` constructor: the
    /// painting hung on `face` of the wall block `tile`, with a random motive
    /// among those that fit, or `None` when the face is a top or bottom or
    /// nothing fits.
    pub fn hang(
        chunks: &WorldChunks,
        tile: IVec3,
        face: BlockFace,
        others: &[Aabb],
        rng: &mut ItemRng,
    ) -> Option<Self> {
        let direction = Self::direction_for(face)?;
        let fits: Vec<Art> = Art::all()
            .filter(|&art| {
                Self::new(tile, direction, art).on_valid_surface(chunks, others.iter().copied())
            })
            .collect();
        if fits.is_empty() {
            return None;
        }
        let pick = ((rng.unit() * fits.len() as f32) as usize).min(fits.len() - 1);
        Some(Self::new(tile, direction, fits[pick]))
    }
}

/// `getArtSize`: a motive 32 or 64 pixels across sits half a block over.
/// Beta leaves 48 (the tall ones' height) at zero.
fn art_size(pixels: u16) -> f32 {
    if pixels == 32 || pixels == 64 {
        0.5
    } else {
        0.0
    }
}

pub fn spawn_painting(commands: &mut Commands, painting: Painting) -> Entity {
    let transform = Transform::from_translation(painting.center())
        .with_rotation(Quat::from_rotation_y(painting.yaw().to_radians()));
    commands
        .spawn((
            Name::new("Painting"),
            painting,
            transform,
            Visibility::default(),
        ))
        .id()
}

/// `attackEntityFrom` and the failed wall check: the painting is gone and
/// its item drops where it hung.
pub fn break_painting(commands: &mut Commands, rng: &mut ItemRng, entity: Entity, center: Vec3) {
    commands.entity(entity).despawn();
    if let Ok(stack) = ItemStack::new(Item::Painting, 1) {
        spawn_constructed_drop(commands, rng, center, stack);
    }
}

/// `EntityPainting.onUpdate`: every 101st tick, check the wall.
pub(crate) fn tick_paintings(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mut edit: WorldEdit,
    mut paintings: Query<(Entity, &mut Painting, &Transform)>,
    mut rng: Local<ItemRng>,
    mut boxes: Local<Vec<(Entity, Aabb)>>,
    mut fallen: Local<Vec<Entity>>,
) {
    let count = tick.ticks_this_frame();
    if count == 0 {
        return;
    }
    boxes.clear();
    boxes.extend(
        paintings
            .iter()
            .map(|(entity, painting, _)| (entity, painting.aabb())),
    );
    fallen.clear();
    let mut order: Vec<Entity> = boxes.iter().map(|(entity, _)| *entity).collect();
    order.sort();
    for _ in 0..count {
        for &entity in &order {
            if fallen.contains(&entity) {
                continue;
            }
            let Ok((_, mut painting, transform)) = paintings.get_mut(entity) else {
                continue;
            };
            let center = transform.translation;
            if !edit
                .chunks
                .contains(ChunkPosition::from_world(center.x, center.z))
            {
                continue;
            }
            let ticks = painting.ticks;
            painting.ticks = ticks.saturating_add(1);
            if ticks != CHECK_TICKS {
                continue;
            }
            painting.ticks = 0;
            let others = boxes
                .iter()
                .filter(|(other, _)| *other != entity && !fallen.contains(other))
                .map(|(_, aabb)| *aabb);
            if !painting.on_valid_surface(&edit.chunks, others) {
                fallen.push(entity);
                break_painting(&mut commands, &mut rng, entity, center);
                edit.mark_dirty(ChunkPosition::from_world(center.x, center.z));
            }
        }
    }
}
