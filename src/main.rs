#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

use bevy::prelude::*;

mod game;

fn main() {
    App::build().add_plugin(game::GamePlugin).run();
}
