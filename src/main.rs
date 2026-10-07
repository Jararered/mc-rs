use bevy::app::TaskPoolThreadAssignmentPolicy;
use bevy::image::ImagePlugin;
use bevy::image::ImageSamplerDescriptor;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::MemoryHints;
use bevy::render::settings::WgpuSettings;
use game::app::GamePlugin;
use std::sync::Arc;

/// Lets the main and render threads take a core from a chunk worker whenever
/// they want one. Generation and meshing fill every async compute thread
/// while terrain streams in, and at equal priority they share the frame's
/// cores evenly.
fn lower_thread_priority() {
    #[cfg(target_os = "linux")]
    // SAFETY: both calls only act on the calling thread's scheduling.
    unsafe {
        // Linux applies a `PRIO_PROCESS` niceness to the one thread named.
        if let Ok(thread) = libc::id_t::try_from(libc::gettid()) {
            libc::setpriority(libc::PRIO_PROCESS, thread, 10);
        }
    }
    #[cfg(target_os = "macos")]
    // SAFETY: only changes the calling thread's quality-of-service class.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
}

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(bevy::pbr::PbrPlugin {
                    add_default_deferred_lighting_plugin: false,
                    ..default()
                })
                .set(ImagePlugin {
                    // Pixel art: nearest sampling and no mip chain. A per-image sampler
                    // replaces this, so loaders should leave `ImageSampler::Default` alone
                    // unless they need a different address mode.
                    default_sampler: ImageSamplerDescriptor {
                        lod_max_clamp: 0.0,
                        ..ImageSamplerDescriptor::nearest()
                    },
                })
                .set(RenderPlugin {
                    render_creation: WgpuSettings {
                        // wgpu's default reserves large blocks for its Vulkan
                        // and DX12 allocators; the renderer here holds a few
                        // big buffers and little else. Metal has no such
                        // allocator, so this changes nothing on macOS.
                        memory_hints: MemoryHints::MemoryUsage,
                        ..default()
                    }
                    .into(),
                    ..default()
                })
                .set(TaskPoolPlugin {
                    task_pool_options: TaskPoolOptions {
                        // Chunk generation, lighting, and meshing run on the
                        // async compute pool. Bevy's default gives it a quarter
                        // of the cores, capped at four threads. The saves on
                        // the IO pool keep normal priority.
                        async_compute: TaskPoolThreadAssignmentPolicy {
                            min_threads: 1,
                            max_threads: 8,
                            percent: 0.4,
                            on_thread_spawn: Some(Arc::new(lower_thread_priority)),
                            on_thread_destroy: None,
                        },
                        ..default()
                    },
                }),
        )
        .add_plugins(GamePlugin)
        .run();
}
