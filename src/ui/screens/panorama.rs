//! Optional rotating title panorama, drawn behind the main-menu UI.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::image::ImageSampler;
use bevy::image::ImageSamplerDescriptor;
use bevy::light::Skybox;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use bevy::render::render_resource::TextureViewDescriptor;
use bevy::render::render_resource::TextureViewDimension;

use crate::app::state::AppScreen;

/// Keep this camera separate from the world cameras, which are disabled in menus.
#[derive(Component)]
pub(crate) struct MenuPanoramaCamera;

#[derive(Component)]
pub(super) struct MenuPanoramaRoot;

#[derive(Resource)]
struct PanoramaFaces {
    images: [Handle<Image>; 6],
    cubemap: Option<Handle<Image>>,
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(PreStartup, load_faces)
        .add_systems(Startup, spawn_camera)
        .add_systems(
            Update,
            (assemble_cubemap, show_panorama, rotate_camera).chain(),
        );
}

fn load_faces(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(PanoramaFaces {
        images: std::array::from_fn(|i| assets.load(format!("title/bg/panorama{i}.png"))),
        cubemap: None,
    });
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Title panorama camera"),
        MenuPanoramaCamera,
        // Do not draw world chunks or the first-person arm over the skybox.
        RenderLayers::layer(4),
        Camera3d::default(),
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        Camera {
            order: 1, // World at 0, panorama at 1, UI at 2.
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.18, 0.15, 0.13)),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 70.0_f32.to_radians(),
            ..default()
        }),
        Skybox {
            // Bevy multiplies skybox luminance by camera exposure (about 0.001
            // at the default EV100); values near 1 render almost black.
            brightness: 850.0,
            ..default()
        },
    ));
}

// Cube layers are +X, -X, +Y, -Y, +Z, -Z. The reference panorama's
// horizontal faces run front -> right -> back -> left (0, 1, 2, 3).
const CUBE_FACES: [usize; 6] = [1, 3, 4, 5, 0, 2];

fn assemble_cubemap(
    mut faces: ResMut<PanoramaFaces>,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut cameras: Query<&mut Skybox, With<MenuPanoramaCamera>>,
) {
    if faces.cubemap.is_some()
        || !faces
            .images
            .iter()
            .all(|image| assets.is_loaded(image.id()))
    {
        return;
    }
    let Some(source) = images.get(&faces.images[0]) else {
        return;
    };
    let size = source.texture_descriptor.size;
    let format = source.texture_descriptor.format;
    if size.width != 256 || size.height != 256 || size.depth_or_array_layers != 1 {
        return;
    }
    let Some(face_len) = source.data.as_ref().map(Vec::len) else {
        return;
    };
    if !matches!(
        format,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
    ) || face_len != size.width as usize * size.height as usize * 4
    {
        return;
    }
    let mut pixels = Vec::with_capacity(face_len * 6);
    for index in CUBE_FACES {
        let Some(face) = images.get(&faces.images[index]) else {
            return;
        };
        if face.texture_descriptor.size != size || face.texture_descriptor.format != format {
            return;
        }
        let Some(data) = face.data.as_ref().filter(|data| data.len() == face_len) else {
            return;
        };
        if index == 4 || index == 5 {
            // The top and bottom PNGs are upside down relative to cubemap UVs.
            let row_bytes = face_len / size.height as usize;
            for row in data.chunks_exact(row_bytes).rev() {
                pixels.extend_from_slice(row);
            }
        } else {
            pixels.extend_from_slice(data);
        }
    }
    // Blur once at load time; unlike terrain tiles, the panorama is meant to be
    // soft when its 256px faces fill the screen.
    blur_faces(&mut pixels, size.width as usize, size.height as usize);
    let mut cubemap = Image::new(
        Extent3d {
            depth_or_array_layers: 6,
            ..size
        },
        TextureDimension::D2,
        pixels,
        format,
        RenderAssetUsages::default(),
    );
    // This menu-only image intentionally overrides the pixel-art sampler.
    cubemap.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        lod_max_clamp: 0.0,
        ..ImageSamplerDescriptor::linear()
    });
    cubemap.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    let handle = images.add(cubemap);
    for mut skybox in &mut cameras {
        skybox.image = Some(handle.clone());
    }
    faces.cubemap = Some(handle);
}

/// Separable 5-tap Gaussian ([1, 4, 6, 4, 1] / 16), clamped at face edges.
/// Reuse the scratch buffer across all six faces and leave the source assets intact.
fn blur_faces(pixels: &mut [u8], width: usize, height: usize) {
    const WEIGHTS: [u32; 5] = [1, 4, 6, 4, 1];
    let face_len = width * height * 4;
    let mut horizontal = vec![0_u8; face_len];
    for face in pixels.chunks_exact_mut(face_len) {
        for y in 0..height {
            for x in 0..width {
                for channel in 0..4 {
                    let sum: u32 = WEIGHTS
                        .into_iter()
                        .enumerate()
                        .map(|(tap, weight)| {
                            let sample_x = (x as isize + tap as isize - 2)
                                .clamp(0, width as isize - 1)
                                as usize;
                            weight * u32::from(face[(y * width + sample_x) * 4 + channel])
                        })
                        .sum();
                    horizontal[(y * width + x) * 4 + channel] = ((sum + 8) / 16) as u8;
                }
            }
        }
        for y in 0..height {
            for x in 0..width {
                for channel in 0..4 {
                    let sum: u32 = WEIGHTS
                        .into_iter()
                        .enumerate()
                        .map(|(tap, weight)| {
                            let sample_y = (y as isize + tap as isize - 2)
                                .clamp(0, height as isize - 1)
                                as usize;
                            weight * u32::from(horizontal[(sample_y * width + x) * 4 + channel])
                        })
                        .sum();
                    face[(y * width + x) * 4 + channel] = ((sum + 8) / 16) as u8;
                }
            }
        }
    }
}

fn show_panorama(
    mut commands: Commands,
    screen: Res<State<AppScreen>>,
    faces: Res<PanoramaFaces>,
    mut cameras: Query<&mut Camera, With<MenuPanoramaCamera>>,
    mut roots: Query<(Entity, Option<&ImageNode>, &mut BackgroundColor), With<MenuPanoramaRoot>>,
) {
    let active = *screen.get() == AppScreen::Menu && faces.cubemap.is_some();
    for mut camera in &mut cameras {
        if camera.is_active != active {
            camera.is_active = active;
        }
    }
    if active {
        for (entity, fallback, mut color) in &mut roots {
            if fallback.is_some() {
                commands.entity(entity).remove::<ImageNode>();
            }
            let tint = Color::srgba(0.0, 0.0, 0.0, 0.38);
            if color.0 != tint {
                color.0 = tint;
            }
        }
    }
}

fn rotate_camera(
    time: Res<Time>,
    screen: Res<State<AppScreen>>,
    mut cameras: Query<&mut Transform, With<MenuPanoramaCamera>>,
) {
    if *screen.get() != AppScreen::Menu {
        return;
    }
    for mut transform in &mut cameras {
        transform.rotation = Quat::from_euler(
            EulerRot::YXZ,
            time.elapsed_secs() * 0.012,
            (-10.0 + 3.0 * (time.elapsed_secs() * 0.15).sin()).to_radians(),
            0.0,
        );
    }
}
