#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

use bevy::prelude::*;

mod bonus;
mod bullet;
mod ff;
mod game;
mod level;
mod player;
mod render;
mod robot;

fn main() {
    App::build().add_plugin(game::GamePlugin).run();
}
