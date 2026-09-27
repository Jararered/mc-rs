//! Minecart motion on Beta rail shapes, including powered-rail acceleration.
use bevy::prelude::*;

use crate::block::id::Id;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::physics::Aabb;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::tick::WorldTick;

pub const CART_SIZE: EntitySize = EntitySize {
    width: 0.98,
    height: 0.7,
    y_offset: 0.35,
};

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Minecart {
    /// Blocks per world tick on the horizontal track plane.
    pub motion: Vec3,
}

#[derive(Component)]
pub(crate) struct CartVisual;
#[derive(Component)]
pub(crate) struct CartPiece(Vec3);

pub fn spawn_minecart(commands: &mut Commands, cell: IVec3) {
    let center = cell.as_vec3() + Vec3::new(0.5, CART_SIZE.y_offset, 0.5);
    commands.spawn((
        Name::new("Minecart"),
        Minecart::default(),
        CART_SIZE,
        PreviousTick(center),
        Transform::from_translation(center),
        Visibility::default(),
    ));
}

/// First ray intersection distance with a cart-sized or block-sized box.
pub fn ray_box_distance(origin: Vec3, direction: Vec3, bounds: Aabb, reach: f32) -> Option<f32> {
    let mut near: f32 = 0.0;
    let mut far = reach;
    for axis in 0..3 {
        let speed = direction[axis];
        if speed.abs() < f32::EPSILON {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return None;
            }
        } else {
            let a = (bounds.min[axis] - origin[axis]) / speed;
            let b = (bounds.max[axis] - origin[axis]) / speed;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    Some(near)
}

fn rail(block: Id) -> bool {
    matches!(block, Id::Rail | Id::PoweredRail | Id::DetectorRail)
}

/// Track endpoints at half-cell offsets. Beta's `EntityMinecart.MATRIX`
/// establishes the same ten shapes; the second tuple entry is endpoint rise.
fn path(shape: u8) -> (Vec3, Vec3) {
    match shape {
        1 => (Vec3::new(-0.5, 0.0, 0.0), Vec3::new(0.5, 0.0, 0.0)),
        2 => (Vec3::new(-0.5, 0.0, 0.0), Vec3::new(0.5, 1.0, 0.0)),
        3 => (Vec3::new(-0.5, 1.0, 0.0), Vec3::new(0.5, 0.0, 0.0)),
        4 => (Vec3::new(0.0, 1.0, -0.5), Vec3::new(0.0, 0.0, 0.5)),
        5 => (Vec3::new(0.0, 0.0, -0.5), Vec3::new(0.0, 1.0, 0.5)),
        6 => (Vec3::new(0.0, 0.0, 0.5), Vec3::new(0.5, 0.0, 0.0)),
        7 => (Vec3::new(0.0, 0.0, 0.5), Vec3::new(-0.5, 0.0, 0.0)),
        8 => (Vec3::new(0.0, 0.0, -0.5), Vec3::new(-0.5, 0.0, 0.0)),
        9 => (Vec3::new(0.0, 0.0, -0.5), Vec3::new(0.5, 0.0, 0.0)),
        _ => (Vec3::new(0.0, 0.0, -0.5), Vec3::new(0.0, 0.0, 0.5)),
    }
}

fn rail_at(chunks: &WorldChunks, center: Vec3) -> Option<(IVec3, Id)> {
    let x = center.x.floor() as i32;
    let z = center.z.floor() as i32;
    let y = (center.y - CART_SIZE.y_offset).round() as i32;
    for dy in [0, 1, -1] {
        let cell = IVec3::new(x, y + dy, z);
        if let Some(block) = chunks.block_at(cell.x, cell.y, cell.z)
            && rail(block)
        {
            return Some((cell, block));
        }
    }
    None
}

/// One 20 Hz step, clamped to the current rail segment and its next cell.
pub fn step_minecart(cart: &mut Minecart, center: &mut Vec3, chunks: &WorldChunks) {
    let Some((cell, block)) = rail_at(chunks, *center) else {
        cart.motion = Vec3::ZERO;
        return;
    };
    let shape = chunks.metadata_at(cell.x, cell.y, cell.z) & 7;
    let (start, end) = path(shape);
    let base = cell.as_vec3() + Vec3::new(0.5, CART_SIZE.y_offset, 0.5);
    let a = base + start;
    let b = base + end;
    let horizontal = Vec3::new(b.x - a.x, 0.0, b.z - a.z).normalize();
    let mut speed = cart.motion.dot(horizontal);
    if block == Id::PoweredRail {
        if chunks.metadata_at(cell.x, cell.y, cell.z) & 8 == 0 {
            speed *= 0.5;
        } else if speed.abs() >= 0.01 {
            speed += speed.signum() * 0.06;
        } else {
            let behind = cell - horizontal.as_ivec3();
            let ahead = cell + horizontal.as_ivec3();
            if chunks
                .block_at(behind.x, behind.y, behind.z)
                .is_some_and(crate::block::properties::is_opaque_cube)
            {
                speed = 0.02;
            } else if chunks
                .block_at(ahead.x, ahead.y, ahead.z)
                .is_some_and(crate::block::properties::is_opaque_cube)
            {
                speed = -0.02;
            }
        }
    }
    if (2..=5).contains(&shape) {
        // Gravity accelerates toward the low end of a slope.
        speed -= (end.y - start.y) * 0.0078;
    }
    speed = (speed * 0.997).clamp(-0.4, 0.4);
    let delta = b - a;
    let progress = (((*center - a).x * delta.x + (*center - a).z * delta.z)
        / (delta.x * delta.x + delta.z * delta.z))
        .clamp(0.0, 1.0);
    let next = progress + speed / delta.xz().length();
    if !(0.0..=1.0).contains(&next) {
        let endpoint = if next < 0.0 { a } else { b };
        let remaining = (next - next.clamp(0.0, 1.0)) * delta.xz().length();
        let candidate = endpoint + horizontal * remaining;
        if rail_at(chunks, candidate).is_some() {
            *center = candidate;
        } else {
            *center = endpoint - horizontal * (0.001 * speed.signum());
            speed = 0.0;
        }
    } else {
        *center = a + delta * next;
    }
    cart.motion = horizontal * speed;
}

pub(crate) fn tick_minecarts(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    mut carts: Query<(&mut Minecart, &mut Transform, &mut PreviousTick)>,
) {
    for (mut cart, mut transform, mut previous) in &mut carts {
        if !chunks.contains(ChunkPosition::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            continue;
        }
        for _ in 0..tick.ticks_this_frame() {
            previous.0 = transform.translation;
            let mut center = transform.translation;
            step_minecart(&mut cart, &mut center, &chunks);
            transform.translation = center;
        }
    }
}

pub(crate) fn sync_minecart_rendering(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    new_carts: Query<Entity, (With<Minecart>, Without<CartVisual>)>,
    visuals: Query<(&Transform, &PreviousTick, &Children), (With<Minecart>, With<CartVisual>)>,
    mut pieces: Query<(&mut Transform, &CartPiece), Without<Minecart>>,
) {
    if let Some(materials) = materials.as_deref_mut() {
        for entity in &new_carts {
            let iron = materials.add(StandardMaterial {
                base_color: Color::srgb(0.5, 0.53, 0.52),
                metallic: 0.3,
                perceptual_roughness: 0.8,
                ..default()
            });
            let mut children = Vec::new();
            for (size, offset) in [
                (Vec3::new(0.9, 0.1, 0.9), Vec3::new(0.0, -0.29, 0.0)),
                (Vec3::new(0.9, 0.32, 0.08), Vec3::new(0.0, -0.09, -0.43)),
                (Vec3::new(0.9, 0.32, 0.08), Vec3::new(0.0, -0.09, 0.43)),
                (Vec3::new(0.08, 0.32, 0.9), Vec3::new(-0.43, -0.09, 0.0)),
                (Vec3::new(0.08, 0.32, 0.9), Vec3::new(0.43, -0.09, 0.0)),
            ] {
                children.push(
                    commands
                        .spawn((
                            Mesh3d(meshes.add(Cuboid::from_size(size))),
                            MeshMaterial3d(iron.clone()),
                            Transform::from_translation(offset),
                            CartPiece(offset),
                        ))
                        .id(),
                );
            }
            commands
                .entity(entity)
                .add_children(&children)
                .insert(CartVisual);
        }
    }
    for (transform, previous, children) in &visuals {
        let slide = previous.0.lerp(transform.translation, tick.partial()) - transform.translation;
        for child in children.iter() {
            if let Ok((mut piece, offset)) = pieces.get_mut(child) {
                piece.translation = offset.0 + slide;
            }
        }
    }
}
