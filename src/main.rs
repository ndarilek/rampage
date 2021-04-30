#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

use bevy::prelude::*;

mod bonus;
mod bullet;
mod game;
mod level;
mod player;
mod robot;
mod sentry;

fn main() {
    App::build()
        .add_plugin(sentry::SentryPlugin)
        .add_plugin(game::GamePlugin)
        .run();
}
