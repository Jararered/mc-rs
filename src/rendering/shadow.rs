//! Blob shadows under entities, matching Beta `Render.renderShadow`.
//!
//! Beta rasterizes the shadow across every lit, opaque-cube-topped cell
//! within `shadowSize` of the entity, each cell sampling `shadow.png` so the
//! blob follows uneven terrain (`Render.renderShadowOnBlock`). Every entity
//! rendered today (dropped items, `RenderItem.shadowSize = 0.15`) has a
//! shadow well under half a block wide, so that raster is in practice always
//! the single cell beneath the entity's feet. This renders that cell as one
//! quad rather than building a multi-cell raster for a case that cannot
//! happen yet. Revisit with a proper raster if an entity with a wider shadow
//! (Beta's mobs and players use `shadowSize = 0.5`, wide enough to straddle
//! a block corner on uneven ground) is added.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::Indices;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Shadow;
use crate::physics::PhysicsSet;
use crate::player::Player;
use crate::rendering::textures::TintedMaterial;
use crate::rendering::textures::tint_tag;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::beta_brightness;
use crate::world::lighting::light_level_at;
use crate::world::tick::WorldTick;

/// Beta shadows fade out at 16 blocks (`Render.doRenderShadowAndFire`:
/// `1.0 - distanceSquared / 256.0`).
const FADE_DISTANCE_SQUARED: f32 = 256.0;
/// `Render.renderShadow`'s `var6`: keeps the quad from sharing the ground
/// block's own top face.
const SURFACE_OFFSET: f32 = 1.0 / 64.0;
/// `misc/shadow.png`, the same blob texture Beta binds in `renderShadow`.
const SHADOW_TEXTURE: &str = "misc/shadow.png";

/// Links a shadow-casting entity to the quad that renders its blob. Not a
/// scene-graph parent/child: the quad sits on the ground, not wherever the
/// owner's own rotation and scale would place it, so its `Transform` is
/// always set in world space.
#[derive(Component)]
struct ShadowQuad(Entity);

/// Back-reference so a quad can be despawned once its owner is gone.
#[derive(Component)]
pub(crate) struct ShadowOwner(Entity);

#[derive(Resource)]
struct ShadowAssets {
    mesh: Handle<Mesh>,
    material: Handle<TintedMaterial>,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(PostStartup, load_shadow_assets)
        .add_systems(
            Update,
            (
                despawn_orphaned_shadow_quads,
                spawn_shadow_quads,
                update_shadow_quads,
            )
                .chain()
                .after(PhysicsSet::Integrate)
                .run_if(in_state(AppScreen::Playing)),
        );
}

fn load_shadow_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<TintedMaterial>>>,
) {
    let Some(mut meshes) = meshes else {
        return;
    };
    let Some(mut materials) = materials else {
        return;
    };
    let texture = std::path::Path::new("assets")
        .join(SHADOW_TEXTURE)
        .exists()
        .then(|| asset_server.load(SHADOW_TEXTURE.to_string()));
    let material = materials.add(TintedMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: texture,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            ..default()
        },
        extension: default(),
    });
    commands.insert_resource(ShadowAssets {
        mesh: meshes.add(shadow_quad_mesh()),
        material,
    });
}

/// A unit square on the XZ plane, scaled per entity by `2 * Shadow::radius`.
fn shadow_quad_mesh() -> Mesh {
    let h = 0.5;
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-h, 0.0, -h], [-h, 0.0, h], [h, 0.0, h], [h, 0.0, -h]],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
    )
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

fn despawn_orphaned_shadow_quads(
    mut commands: Commands,
    quads: Query<(Entity, &ShadowOwner)>,
    owners: Query<(), With<Shadow>>,
) {
    for (quad, owner) in &quads {
        if owners.get(owner.0).is_err() {
            commands.entity(quad).despawn();
        }
    }
}

fn spawn_shadow_quads(
    mut commands: Commands,
    assets: Option<Res<ShadowAssets>>,
    owners: Query<Entity, (With<Shadow>, Without<ShadowQuad>)>,
) {
    let Some(assets) = assets else {
        return;
    };
    for owner in &owners {
        let quad = commands
            .spawn((
                Name::new("Entity shadow"),
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.material.clone()),
                tint_tag(Color::NONE),
                Transform::default(),
                Visibility::Hidden,
                ShadowOwner(owner),
                NoFrustumCulling,
            ))
            .id();
        // The owner can be despawned by a system that ran alongside this one
        // (an item burning up on its first tick); the quad is then an orphan
        // for `despawn_orphaned_shadow_quads`.
        commands.entity(owner).try_insert(ShadowQuad(quad));
    }
}

fn update_shadow_quads(
    settings: Res<GameSettings>,
    tick: Res<WorldTick>,
    environment: crate::world::dimension::Environment,
    chunks: Res<WorldChunks>,
    camera: Query<&Transform, With<Player>>,
    owners: Query<(
        &Shadow,
        &Transform,
        &EntitySize,
        &ShadowQuad,
        Option<&PreviousTick>,
    )>,
    mut quads: Query<
        (&mut Transform, &mut MeshTag, &mut Visibility),
        (With<ShadowOwner>, Without<Player>, Without<Shadow>),
    >,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    let hidden = !settings.graphics.entity_shadows();
    let subtracted = environment.skylight_subtracted(tick.partial());
    for (shadow, transform, size, link, previous_tick) in &owners {
        let Ok((mut quad_transform, mut tag, mut visibility)) = quads.get_mut(link.0) else {
            continue;
        };
        // Beta interpolates the shadow's own anchor from `lastTickPosX/Y/Z`
        // rather than reading the raw, tick-quantized position, so it glides
        // with the entity's rendered position instead of stepping once a
        // tick.
        let position = previous_tick.map_or(transform.translation, |previous| {
            previous
                .0
                .lerp(transform.translation, tick.partial().clamp(0.0, 1.0))
        });
        let placed = (!hidden)
            .then(|| {
                place_shadow(
                    shadow,
                    position,
                    size,
                    camera.translation,
                    &chunks,
                    subtracted,
                )
            })
            .flatten();
        match placed {
            Some((position, alpha)) => {
                quad_transform.set_if_neq(
                    Transform::from_translation(position).with_scale(Vec3::new(
                        shadow.radius * 2.0,
                        1.0,
                        shadow.radius * 2.0,
                    )),
                );
                tag.set_if_neq(tint_tag(Color::WHITE.with_alpha(alpha)));
                visibility.set_if_neq(Visibility::Inherited);
            }
            None => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

/// `Render.renderShadow` + `renderShadowOnBlock`, simplified to the single
/// ground cell under the entity (see module doc). `position` is the
/// entity's own render position (already interpolated between ticks, if it
/// tracks a [`PreviousTick`]). Returns the quad's world position and alpha,
/// or `None` if nothing should be drawn.
pub fn place_shadow(
    shadow: &Shadow,
    position: Vec3,
    size: &EntitySize,
    camera: Vec3,
    chunks: &WorldChunks,
    skylight_subtracted: u8,
) -> Option<(Vec3, f32)> {
    let distance_sq = position.distance_squared(camera);
    let fade = (1.0 - distance_sq / FADE_DISTANCE_SQUARED) * shadow.opacity_scale;
    if fade <= 0.0 {
        return None;
    }

    let feet_y = position.y - size.y_offset;
    let cell_x = position.x.floor() as i32;
    let cell_z = position.z.floor() as i32;
    let ground_cell_y = feet_y.floor() as i32;

    let support = chunks.block_at(cell_x, ground_cell_y - 1, cell_z)?;
    if !support.is_opaque_cube() {
        return None;
    }
    let light = light_level_at(chunks, cell_x, ground_cell_y, cell_z, skylight_subtracted);
    if light <= 3 {
        return None;
    }

    // `Entity.getShadowSize()` defaults to half the entity's height; added to
    // its feet, this is the anchor Beta measures the shadow's height falloff
    // from.
    let shadow_anchor_y = feet_y + size.height * 0.5;
    let ground_top_y = ground_cell_y as f32;
    let height_above_ground = (shadow_anchor_y - ground_top_y).max(0.0);
    let alpha = (fade - height_above_ground / 2.0)
        * 0.5
        * beta_brightness(
            light,
            crate::world::dimension::Dimension::Overworld.ambient_light(),
        );
    if alpha <= 0.0 {
        return None;
    }

    let world_position = Vec3::new(position.x, ground_top_y + SURFACE_OFFSET, position.z);
    Some((world_position, alpha.min(1.0)))
}
