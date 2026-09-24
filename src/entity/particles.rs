//! Short-lived, terrain-atlas block debris. One dynamic mesh renders the
//! bounded particle pool; blocks themselves never become entities.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::state::AppScreen;
use crate::physics::BlockFace;
use crate::physics::BlockHit;
use crate::physics::PhysicsSet;
use crate::player::PlayerCamera;
use crate::world::block::block::BlockId;
use crate::world::block::properties::blocks_movement;
use crate::world::chunk::WorldChunks;
use crate::world::generation::Climate;
use crate::world::textures::FoliageColors;
use crate::world::textures::GrassColors;
use crate::world::textures::TerrainMaterial;
use crate::world::textures::atlas_tile_uvs;
use crate::world::textures::block_tile;
use crate::world::tick::WorldTick;

const MAX_PARTICLES: usize = 4_000;
const BURST_SIDE: usize = 4;

pub struct BlockParticlePlugin;

impl Plugin for BlockParticlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockParticles>()
            .init_asset::<StandardMaterial>()
            .add_systems(Startup, setup_renderer)
            .add_systems(
                Update,
                (sync_terrain_texture, update_particles)
                    .after(PhysicsSet::ApplyInput)
                    .run_if(in_state(AppScreen::Playing)),
            );
    }
}

#[derive(Clone, Copy, Debug)]
enum ParticleRequest {
    Break(BlockHit),
    Hit(BlockHit),
}

/// Queue effects only after a successful world edit. The pool is capped to
/// Beta's 4,000 terrain particles, dropping the oldest when full.
#[derive(Resource)]
pub struct BlockParticles {
    requests: Vec<ParticleRequest>,
    active: VecDeque<Particle>,
    random: u64,
}

impl Default for BlockParticles {
    fn default() -> Self {
        Self {
            requests: Vec::new(),
            active: VecDeque::new(),
            random: 0x9e37_79b9_7f4a_7c15,
        }
    }
}

impl BlockParticles {
    pub fn emit_break(&mut self, hit: BlockHit) {
        self.requests.push(ParticleRequest::Break(hit));
    }

    pub fn emit_hit(&mut self, hit: BlockHit) {
        self.requests.push(ParticleRequest::Hit(hit));
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    fn random(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        (self.random >> 40) as f32 / (1_u32 << 24) as f32
    }

    fn random_centered(&mut self) -> f32 {
        self.random() * 0.8 - 0.4
    }

    fn push(&mut self, particle: Particle) {
        if self.active.len() == MAX_PARTICLES {
            self.active.pop_front();
        }
        self.active.push_back(particle);
    }

    fn spawn_pending(
        &mut self,
        chunks: &WorldChunks,
        foliage: Option<&FoliageColors>,
        grass: Option<&GrassColors>,
    ) {
        let requests = std::mem::take(&mut self.requests);
        for request in requests {
            match request {
                ParticleRequest::Break(hit) => self.spawn_break(hit, chunks, foliage, grass),
                ParticleRequest::Hit(hit) => self.spawn_hit(hit, chunks, foliage, grass),
            }
        }
    }

    fn spawn_break(
        &mut self,
        hit: BlockHit,
        chunks: &WorldChunks,
        foliage: Option<&FoliageColors>,
        grass: Option<&GrassColors>,
    ) {
        let climate = chunks.climate_at(hit.x, hit.z);
        for x in 0..BURST_SIDE {
            for y in 0..BURST_SIDE {
                for z in 0..BURST_SIDE {
                    let offset = (Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5))
                        / BURST_SIDE as f32;
                    let position = Vec3::new(hit.x as f32, hit.y as f32, hit.z as f32) + offset;
                    let outward = offset - Vec3::splat(0.5);
                    let particle = self
                        .new_particle(position, outward, hit.block, climate, foliage, grass, 1.0);
                    self.push(particle);
                }
            }
        }
    }

    fn spawn_hit(
        &mut self,
        hit: BlockHit,
        chunks: &WorldChunks,
        foliage: Option<&FoliageColors>,
        grass: Option<&GrassColors>,
    ) {
        let climate = chunks.climate_at(hit.x, hit.z);
        let mut offset = Vec3::new(
            0.1 + self.random() * 0.8,
            0.1 + self.random() * 0.8,
            0.1 + self.random() * 0.8,
        );
        match hit.face {
            BlockFace::Down => offset.y = -0.1,
            BlockFace::Up => offset.y = 1.1,
            BlockFace::North => offset.z = -0.1,
            BlockFace::South => offset.z = 1.1,
            BlockFace::West => offset.x = -0.1,
            BlockFace::East => offset.x = 1.1,
        }
        let position = Vec3::new(hit.x as f32, hit.y as f32, hit.z as f32) + offset;
        let mut particle = self.new_particle(
            position,
            Vec3::ZERO,
            hit.block,
            climate,
            foliage,
            grass,
            0.6,
        );
        particle.velocity.x *= 0.2;
        particle.velocity.z *= 0.2;
        particle.velocity.y = (particle.velocity.y - 0.1) * 0.2 + 0.1;
        self.push(particle);
    }

    fn new_particle(
        &mut self,
        position: Vec3,
        outward: Vec3,
        block: BlockId,
        climate: Option<Climate>,
        foliage: Option<&FoliageColors>,
        grass: Option<&GrassColors>,
        scale: f32,
    ) -> Particle {
        let direction = outward
            + Vec3::new(
                self.random_centered(),
                self.random_centered(),
                self.random_centered(),
            );
        let speed = (1.0 + self.random() + self.random()) * 0.15 * 0.4;
        let velocity = direction.normalize_or_zero() * speed + Vec3::Y * 0.1;
        let jitter = Vec2::new(self.random() * 3.0, self.random() * 3.0);
        let size = (0.5 + self.random() * 0.5) * scale * 0.1;
        let max_age = (4.0 / (self.random() * 0.9 + 0.1)) as u8;
        let (tile_x, tile_y) = block_tile(block, 1, true);
        let (u0, v0, u1, v1) = atlas_tile_uvs(tile_x, tile_y);
        let tile_size = Vec2::new(u1 - u0, v1 - v0);
        let patch_min = Vec2::new(u0, v0) + jitter * tile_size * 0.25;
        let patch_max = patch_min + tile_size * 0.24975;
        let block_tint = match block {
            BlockId::TallGrass | BlockId::Fern => Some(grass.map_or_else(
                || GrassColors::default().sample_optional(climate),
                |colors| colors.sample_optional(climate),
            )),
            BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves => {
                climate.map(|climate| foliage.map_or([0.28, 0.71, 0.09], |f| f.sample(climate)))
            }
            _ => None,
        };
        let tint = block_tint.unwrap_or([1.0; 3]);
        Particle {
            position,
            previous_position: position,
            velocity,
            age: 0,
            max_age,
            size,
            uv: [patch_min.x, patch_min.y, patch_max.x, patch_max.y],
            color: [tint[0] * 0.6, tint[1] * 0.6, tint[2] * 0.6, 1.0],
            on_ground: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Particle {
    position: Vec3,
    previous_position: Vec3,
    velocity: Vec3,
    age: u8,
    max_age: u8,
    size: f32,
    uv: [f32; 4],
    color: [f32; 4],
    on_ground: bool,
}

impl Particle {
    fn tick(&mut self, chunks: &WorldChunks) -> bool {
        self.age += 1;
        if self.age > self.max_age {
            return false;
        }
        self.previous_position = self.position;
        self.velocity.y -= 0.04;
        self.on_ground = false;

        let next_x = self.position + Vec3::X * self.velocity.x;
        if !collides(chunks, next_x) {
            self.position.x = next_x.x;
        } else {
            self.velocity.x = 0.0;
        }
        let next_z = self.position + Vec3::Z * self.velocity.z;
        if !collides(chunks, next_z) {
            self.position.z = next_z.z;
        } else {
            self.velocity.z = 0.0;
        }
        let next_y = self.position + Vec3::Y * self.velocity.y;
        if !collides(chunks, next_y - Vec3::Y * 0.08) {
            self.position.y = next_y.y;
        } else {
            self.on_ground = self.velocity.y < 0.0;
            self.velocity.y = 0.0;
        }
        self.velocity *= 0.98;
        if self.on_ground {
            self.velocity.x *= 0.7;
            self.velocity.z *= 0.7;
        }
        true
    }
}

fn collides(chunks: &WorldChunks, point: Vec3) -> bool {
    chunks
        .block_at(
            point.x.floor() as i32,
            point.y.floor() as i32,
            point.z.floor() as i32,
        )
        .is_some_and(blocks_movement)
}

#[derive(Resource)]
struct ParticleRenderer {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    has_geometry: bool,
}

fn setup_renderer(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Bevy 0.19 does not allocate zero-vertex meshes but may still try to
    // upload their data. Keep a nonempty mesh allocated and hide it while idle.
    let mesh = meshes.add(placeholder_mesh());
    let material = materials.add(StandardMaterial {
        alpha_mode: AlphaMode::Mask(0.5),
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let entity = commands
        .spawn((
            Name::new("Block particles"),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Visibility::Hidden,
            NoFrustumCulling,
            NotShadowCaster,
        ))
        .id();
    commands.insert_resource(ParticleRenderer {
        entity,
        mesh,
        material,
        has_geometry: false,
    });
}

fn sync_terrain_texture(
    terrain: Option<Res<TerrainMaterial>>,
    renderer: Res<ParticleRenderer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(terrain) = terrain else { return };
    let texture = materials
        .get(&terrain.0)
        .and_then(|material| material.base_color_texture.clone());
    if let Some(mut material) = materials.get_mut(&renderer.material)
        && material.base_color_texture != texture
    {
        material.base_color_texture = texture;
    }
}

fn update_particles(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    foliage: Option<Res<FoliageColors>>,
    grass: Option<Res<GrassColors>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    mut particles: ResMut<BlockParticles>,
    mut renderer: ResMut<ParticleRenderer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut visibility: Query<&mut Visibility>,
) {
    particles.spawn_pending(&chunks, foliage.as_deref(), grass.as_deref());
    for _ in 0..tick.ticks_this_frame() {
        particles
            .active
            .retain_mut(|particle| particle.tick(&chunks));
    }
    if particles.active.is_empty() {
        if renderer.has_geometry {
            if let Ok(mut visible) = visibility.get_mut(renderer.entity) {
                *visible = Visibility::Hidden;
            }
            renderer.has_geometry = false;
        }
        return;
    }
    let rotation = camera
        .single()
        .map_or(Quat::IDENTITY, GlobalTransform::rotation);
    let right = rotation * Vec3::X;
    let up = rotation * Vec3::Y;
    if let Some(mut mesh) = meshes.get_mut(&renderer.mesh) {
        *mesh = particle_mesh(particles.active.iter(), right, up, tick.partial());
        renderer.has_geometry = !particles.active.is_empty();
        if let Ok(mut visible) = visibility.get_mut(renderer.entity) {
            *visible = Visibility::Visible;
        }
    }
}

fn placeholder_mesh() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0; 4]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0; 2]; 3])
    .with_inserted_indices(Indices::U32(vec![0, 1, 2]))
}

fn particle_mesh<'a>(
    particles: impl ExactSizeIterator<Item = &'a Particle>,
    right: Vec3,
    up: Vec3,
    alpha: f32,
) -> Mesh {
    let count = particles.len();
    let mut positions = Vec::with_capacity(count * 4);
    let mut normals = Vec::with_capacity(count * 4);
    let mut colors = Vec::with_capacity(count * 4);
    let mut uvs = Vec::with_capacity(count * 4);
    let mut indices = Vec::with_capacity(count * 6);
    let normal = right.cross(up);
    for particle in particles {
        let base = positions.len() as u32;
        let r = right * particle.size;
        let u = up * particle.size;
        let center = particle.previous_position.lerp(particle.position, alpha);
        for point in [
            center - r - u,
            center + r - u,
            center + r + u,
            center - r + u,
        ] {
            positions.push(point.to_array());
        }
        normals.extend([normal.to_array(); 4]);
        colors.extend([particle.color; 4]);
        let [u0, v0, u1, v1] = particle.uv;
        uvs.extend([[u0, v1], [u1, v1], [u1, v0], [u0, v0]]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}
