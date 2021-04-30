use bevy::prelude::*;

use crate::{
    game::Reset,
    player::LifeLost,
    robot::{RobotKilled, RobotType},
};

fn log_events(
    mut robot_kills: EventReader<RobotKilled>,
    mut resets: EventReader<Reset>,
    mut player_deaths: EventReader<LifeLost>,
) {
    for RobotKilled(_, robot_type, _, _, _) in robot_kills.iter() {
        let msg = match robot_type {
            RobotType::Dumbass => "Dumbass destroyed",
            RobotType::Jackass => "Jackass destroyed",
            RobotType::Badass => "Badass destroyed",
        };
        sentry::capture_message(msg, sentry::Level::Info);
    }
    for reset in resets.iter() {
        let msg = match reset {
            Reset::NewGame => "New game",
            Reset::NewLevel => "Leveled up",
        };
        sentry::capture_message(msg, sentry::Level::Info);
    }
    for _ in player_deaths.iter() {
        sentry::capture_message("Life lost", sentry::Level::Info);
    }
}

pub struct SentryPlugin;

impl Plugin for SentryPlugin {
    fn build(&self, app: &mut AppBuilder) {
        dotenv::dotenv().ok();
        if let Ok(dsn) = dotenv::var("SENTRY_DSN") {
            let guard = sentry::init(dsn);
            app.insert_resource(guard).add_system(log_events.system());
        }
    }
}
