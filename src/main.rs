use std::error::Error;

use bevy::prelude::*;
use bevy_input_actionmap::{GamepadAxisDirection, InputMap};
use bevy_tts::Tts;

#[macro_use]
mod core;
mod error;
mod log;
mod map;
mod navigation;
mod pathfinding;
mod visibility;

use crate::{
    core::{Coordinates, Player, PointLike},
    error::error_handler,
    navigation::{MaxSpeed, Speed, Velocity},
};

#[bevy_main]
fn main() {
    App::build()
        .add_plugin(crate::error::ErrorPlugin)
        .insert_resource(WindowDescriptor {
            title: "Rampage".into(),
            ..Default::default()
        })
        .insert_resource(bevy::log::LogSettings {
            level: bevy::log::Level::DEBUG,
            // filter: "bevy_ecs=trace".into(),
            ..Default::default()
        })
        .add_plugins(DefaultPlugins)
        .add_plugin(bevy_input_actionmap::ActionPlugin::<String>::default())
        .add_plugin(bevy_openal::OpenAlPlugin)
        .add_plugin(bevy_tts::TtsPlugin)
        .add_plugin(core::CorePlugin)
        .add_plugin(log::LogPlugin)
        .add_plugin(map::MapPlugin)
        .add_plugin(navigation::NavigationPlugin)
        .add_plugin(pathfinding::PathfindingPlugin)
        .add_plugin(visibility::VisibilityPlugin)
        .add_system(bevy::input::system::exit_on_esc_system.system())
        .add_startup_system(setup.system())
        .add_startup_system(spawn_player.system())
        .add_system(speak_info.system().chain(error_handler.system()))
        .run();
}

#[derive(Bundle)]
struct PlayerBundle {
    player: Player,
    coordinates: Coordinates,
    transform: Transform,
    speed: Speed,
    max_speed: MaxSpeed,
    velocity: Velocity,
}

impl Default for PlayerBundle {
    fn default() -> Self {
        Self {
            player: Default::default(),
            coordinates: Default::default(),
            transform: Default::default(),
            speed: Default::default(),
            max_speed: MaxSpeed(12.),
            velocity: Default::default(),
        }
    }
}

const SPEAK_COORDINATES: &str = "SPEAK_COORDINATES";

fn setup(mut input: ResMut<InputMap<String>>) {
    input
        .bind(navigation::ACTION_FORWARD, KeyCode::Up)
        .bind_with_deadzone(
            navigation::ACTION_FORWARD,
            GamepadAxisDirection::LeftStickYPositive,
            0.5,
        )
        .bind(navigation::ACTION_BACKWARD, KeyCode::Down)
        .bind_with_deadzone(
            navigation::ACTION_BACKWARD,
            GamepadAxisDirection::LeftStickYNegative,
            0.5,
        )
        .bind(navigation::ACTION_LEFT, KeyCode::Left)
        .bind_with_deadzone(
            navigation::ACTION_LEFT,
            GamepadAxisDirection::LeftStickXNegative,
            0.5,
        )
        .bind(navigation::ACTION_RIGHT, KeyCode::Right)
        .bind_with_deadzone(
            navigation::ACTION_RIGHT,
            GamepadAxisDirection::LeftStickXPositive,
            0.5,
        )
        .bind(SPEAK_COORDINATES, KeyCode::C);
}

fn spawn_player(mut commands: Commands) {
    commands.spawn().insert_bundle(PlayerBundle::default());
}

fn speak_info(
    input: Res<InputMap<String>>,
    mut tts: ResMut<Tts>,
    player: Query<(&Player, &Coordinates)>,
) -> Result<(), Box<dyn Error>> {
    for (_, coordinates) in player.iter() {
        if input.just_active(SPEAK_COORDINATES) {
            tts.speak(
                format!("({}, {})", coordinates.x_i32(), coordinates.y_i32()),
                true,
            )?;
        }
    }
    Ok(())
}
