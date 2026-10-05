use bevy::app::TaskPoolThreadAssignmentPolicy;
use bevy::image::ImagePlugin;
use bevy::image::ImageSamplerDescriptor;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::MemoryHints;
use bevy::render::settings::WgpuSettings;
use game::app::GamePlugin;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
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
                        // of the cores, capped at four threads.
                        async_compute: TaskPoolThreadAssignmentPolicy {
                            min_threads: 1,
                            max_threads: 8,
                            percent: 0.4,
                            on_thread_spawn: None,
                            on_thread_destroy: None,
                        },
                        ..default()
                    },
                }),
        )
        .add_plugins(GamePlugin)
        .run();
}
