//! Render streamed terrain without the menus and save a screenshot, to
//! compare the chunk layer formats.
//!
//! ```text
//! cargo run --features render_harness --example chunk_render -- quads.png
//! MC_CHUNK_QUADS=off cargo run --features render_harness --example chunk_render -- vertices.png
//! cargo run --features render_harness --example chunk_render -- --diff quads.png vertices.png
//! ```
//!
//! `DIMENSION=nether` renders the Nether instead of the Overworld.
//! `VIEW=1` and up pick other camera positions, and `SMOOTH=off` turns smooth lighting off. `DISTANCE` sets the
//! render distance (8), `SIZE=2560x1440` the window's physical size, and
//! `MSAA=off` turns anti-aliasing off, and
//! `WEATHER=rain` or `WEATHER=thunder` renders under a full storm, and
//! `HOLD=30` keeps the window open that many seconds after the timings so
//! `footprint -p chunk_render` can read its memory. The window presents without
//! VSync, but macOS can still pace it to the display, so treat the frame time
//! printed before exit as a hint only.

use std::ops::Not;
use std::path::PathBuf;

use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::image::CompressedImageFormats;
use bevy::image::ImagePlugin;
use bevy::image::ImageSampler;
use bevy::image::ImageSamplerDescriptor;
use bevy::image::ImageType;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::Screenshot;
use bevy::render::view::window::screenshot::save_to_disk;
use bevy::window::PresentMode;
use bevy::window::WindowResolution;
use game::app::settings::GameSettings;
use game::player::Player;
use game::rendering::WorldRenderingPlugin;
use game::rendering::chunk_quads::ChunkQuads;
use game::world::plugin::WorldPlugin;
use game::world::streaming::WorldStreaming;
use game::world::weather::WorldWeather;

/// Frames with no streaming job in flight before the terrain counts as done.
const SETTLED_FRAMES: u32 = 120;
/// Frames timed after the screenshot.
const TIMED_FRAMES: u32 = 600;

#[derive(Resource)]
struct Capture {
    path: PathBuf,
    settled: u32,
    shot: bool,
    timed: u32,
    seconds: f64,
    /// Seconds left to stay open after the timings are printed.
    hold: Option<f64>,
}

fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|flag| flag == "--diff") {
        diff(&arguments[1], &arguments[2]);
        return;
    }
    let path = arguments
        .first()
        .map_or_else(|| PathBuf::from("chunk_render.png"), PathBuf::from);
    let (width, height) = std::env::var("SIZE")
        .ok()
        .and_then(|size| {
            let (width, height) = size.split_once('x')?;
            Some((width.parse().ok()?, height.parse().ok()?))
        })
        .unwrap_or((960, 540));
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(bevy::pbr::PbrPlugin {
                    add_default_deferred_lighting_plugin: false,
                    ..default()
                })
                .set(ImagePlugin {
                    default_sampler: ImageSamplerDescriptor {
                        lod_max_clamp: 0.0,
                        ..ImageSamplerDescriptor::nearest()
                    },
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "chunk_render".into(),
                        resolution: WindowResolution::new(width, height)
                            .with_scale_factor_override(1.0),
                        present_mode: PresentMode::AutoNoVsync,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .insert_resource(GameSettings {
            render_distance: env_number("DISTANCE").unwrap_or(8),
            max_fps: 0,
            // Moving leaves would differ between two runs.
            wiggle_leaves: false,
            anti_aliasing: std::env::var("MSAA")
                .is_ok_and(|value| value == "off")
                .not(),
            smooth_lighting: std::env::var("SMOOTH")
                .is_ok_and(|value| value == "off")
                .not(),
            ..default()
        })
        .insert_resource(Capture {
            path,
            settled: 0,
            shot: false,
            timed: 0,
            seconds: 0.0,
            hold: None,
        })
        .insert_resource(game::world::dimension::ActiveDimension(
            if std::env::var("DIMENSION").is_ok_and(|value| value == "nether") {
                game::world::dimension::Dimension::Nether
            } else {
                game::world::dimension::Dimension::Overworld
            },
        ))
        .add_plugins((WorldPlugin, WorldRenderingPlugin))
        .add_systems(Startup, set_weather)
        .add_systems(Startup, spawn_view)
        .add_systems(Update, capture)
        .run();
}

fn set_weather(mut weather: ResMut<WorldWeather>) {
    let Ok(kind) = std::env::var("WEATHER") else {
        return;
    };
    let thundering = kind == "thunder";
    *weather = WorldWeather {
        raining: true,
        thundering,
        // Long enough that neither turns off during a run.
        rain_time: 100_000,
        thunder_time: 100_000,
        rain_strength: 1.0,
        thunder_strength: if thundering { 1.0 } else { 0.0 },
        ..default()
    };
}

fn spawn_view(mut commands: Commands) {
    let view = std::env::var("VIEW")
        .ok()
        .and_then(|view| view.parse::<u32>().ok())
        .unwrap_or(0);
    // Looking down keeps the moving clouds out of the picture.
    let (eye, target) = match view {
        0 => (Vec3::new(8.0, 110.0, 8.0), Vec3::new(60.0, 60.0, 50.0)),
        1 => (Vec3::new(8.0, 76.0, 8.0), Vec3::new(30.0, 64.0, 24.0)),
        2 => (Vec3::new(-20.0, 90.0, 30.0), Vec3::new(-70.0, 60.0, -20.0)),
        _ => (Vec3::new(8.0, 140.0, 8.0), Vec3::new(9.0, 60.0, 9.0)),
    };
    commands.spawn((Player, Transform::from_translation(eye)));
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(eye).looking_at(target, Vec3::Y),
    ));
}

fn capture(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    streaming: Option<Res<WorldStreaming>>,
    quads: Option<Res<ChunkQuads>>,
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(streaming) = streaming else {
        return;
    };
    let busy = streaming.generating_job_count()
        + streaming.populating_job_count()
        + streaming.meshing_job_count();
    if !capture.shot {
        capture.settled = if busy == 0 && streaming.rendered_mesh_count() > 0 {
            capture.settled + 1
        } else {
            0
        };
        if capture.settled >= SETTLED_FRAMES {
            capture.shot = true;
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(capture.path.clone()));
        }
        return;
    }
    if let Some(hold) = capture.hold.as_mut() {
        *hold -= time.delta_secs_f64();
        if *hold <= 0.0 {
            exit.write(AppExit::Success);
        }
        return;
    }
    capture.timed += 1;
    capture.seconds += time.delta_secs_f64();
    if capture.timed < TIMED_FRAMES {
        return;
    }
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed)
        .unwrap_or(0.0);
    let mib = |bytes: usize| bytes as f64 / (1024.0 * 1024.0);
    println!(
        "chunk_render: {} layers as {}, {} chunks, {} section layers, {:.2} MiB layer geometry",
        capture.path.display(),
        if streaming.quad_layers() {
            "quad records"
        } else {
            "vertex meshes"
        },
        streaming.rendered_mesh_count(),
        streaming.rendered_layer_count(),
        mib(streaming.mesh_bytes()),
    );
    if let Some(quads) = quads {
        let (reserved, used) = quads.memory();
        println!(
            "chunk_render: quad buffer {:.2} MiB reserved, {:.2} MiB in use",
            mib(reserved),
            mib(used),
        );
    }
    println!(
        "chunk_render: {:.2} ms per frame over {} frames ({fps:.0} fps smoothed)",
        capture.seconds * 1000.0 / f64::from(capture.timed),
        capture.timed,
    );
    capture.hold = Some(env_number("HOLD").unwrap_or(0.0));
}

/// Print how far two screenshots are apart.
fn diff(first: &str, second: &str) {
    let load = |path: &str| {
        let bytes = std::fs::read(path).expect("screenshot file");
        Image::from_buffer(
            &bytes,
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::Default,
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        )
        .expect("png screenshot")
    };
    let (a, b) = (load(first), load(second));
    assert_eq!(a.size(), b.size(), "screenshots differ in size");
    let width = a.width() as usize;
    let (a, b) = (a.data.expect("pixels"), b.data.expect("pixels"));
    let (mut differing, mut largest, mut total) = (0u64, 0u8, 0u64);
    let mut cells = std::collections::BTreeMap::<(usize, usize), u32>::new();
    for (index, (pixel_a, pixel_b)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        let delta = (0..3)
            .map(|channel| pixel_a[channel].abs_diff(pixel_b[channel]))
            .max()
            .unwrap_or(0);
        if delta > 0 {
            differing += 1;
            *cells
                .entry((index % width / 40 * 40, index / width / 40 * 40))
                .or_default() += 1;
            total += u64::from(delta);
            largest = largest.max(delta);
        }
    }
    for ((x, y), count) in cells {
        println!("  {count:>5} in the 40-pixel cell at ({x}, {y})");
    }
    let pixels = (a.len() / 4) as u64;
    println!(
        "{differing} of {pixels} pixels differ ({:.3}%), largest channel difference {largest}, \
         mean over differing pixels {:.1}",
        differing as f64 * 100.0 / pixels as f64,
        if differing == 0 {
            0.0
        } else {
            total as f64 / differing as f64
        },
    );
}
