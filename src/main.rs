use bevy::image::ImagePlugin;
use bevy::image::ImageSamplerDescriptor;
use bevy::prelude::*;
use game::app::GamePlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin {
            // Pixel art: nearest sampling and no mip chain. A per-image sampler
            // replaces this, so loaders should leave `ImageSampler::Default` alone
            // unless they need a different address mode.
            default_sampler: ImageSamplerDescriptor {
                lod_max_clamp: 0.0,
                ..ImageSamplerDescriptor::nearest()
            },
        }))
        .add_plugins(GamePlugin)
        .run();
}
