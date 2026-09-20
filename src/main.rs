use bevy::prelude::*;
use game::app::GamePlugin;

fn main() {
    App::new().add_plugins((DefaultPlugins, GamePlugin)).run();
}
