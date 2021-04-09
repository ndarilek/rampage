#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
use std::error::Error;

use bevy::{
    asset::{HandleId, LoadState},
    prelude::*,
};
use bevy_input_actionmap::{GamepadAxisDirection, InputMap};
use bevy_openal::Listener;
use bevy_tts::Tts;

#[macro_use]
mod core;
mod error;
mod log;
mod map;
mod navigation;
mod pathfinding;
mod sound;
mod visibility;

use crate::{
    core::{Angle, Coordinates, Player, PointLike, Yaw},
    error::error_handler,
    navigation::{MaxSpeed, RotationSpeed, Speed, Velocity},
    sound::{Footstep, FootstepBundle},
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
        .add_plugin(sound::SoundPlugin)
        .add_plugin(visibility::VisibilityPlugin)
        .add_state(AppState::Loading)
        .init_resource::<AssetHandles>()
        .init_resource::<Sfx>()
        .add_system(bevy::input::system::exit_on_esc_system.system())
        .add_startup_system(setup.system().chain(error_handler.system()))
        .add_system_set(
            SystemSet::on_update(AppState::Loading)
                .with_system(load.system().chain(error_handler.system())),
        )
        .add_system_set(SystemSet::on_enter(AppState::InGame).with_system(spawn_player.system()))
        .add_system(speak_info.system().chain(error_handler.system()))
        .run();
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum AppState {
    Loading,
    InGame,
    GameOver,
}

// This asset-handling/loading code needs some cleanup.
#[derive(Clone, Debug, Default)]
struct AssetHandles {
    sfx: Vec<HandleUntyped>,
}

#[derive(Clone, Copy, Debug)]
struct Sfx {
    player_footstep: HandleId,
}

impl Default for Sfx {
    fn default() -> Self {
        Self {
            player_footstep: "sfx/player_footstep.flac".into(),
        }
    }
}

#[derive(Bundle)]
struct PlayerBundle {
    player: Player,
    listener: Listener,
    coordinates: Coordinates,
    yaw: Yaw,
    rotation_speed: RotationSpeed,
    transform: Transform,
    global_transform: GlobalTransform,
    speed: Speed,
    max_speed: MaxSpeed,
    velocity: Velocity,
}

impl Default for PlayerBundle {
    fn default() -> Self {
        Self {
            player: Default::default(),
            listener: Default::default(),
            coordinates: Default::default(),
            yaw: Default::default(),
            rotation_speed: RotationSpeed(Angle::Degrees(45.)),
            transform: Default::default(),
            global_transform: Default::default(),
            speed: Default::default(),
            max_speed: MaxSpeed(12.),
            velocity: Default::default(),
        }
    }
}

const SPEAK_COORDINATES: &str = "SPEAK_COORDINATES";
const SPEAK_HEADING: &str = "SPEAK_HEADING";

fn setup(
    asset_server: Res<AssetServer>,
    mut handles: ResMut<AssetHandles>,
    mut input: ResMut<InputMap<String>>,
) -> Result<(), Box<dyn Error>> {
    handles.sfx = asset_server.load_folder("sfx")?;
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
        .bind(
            navigation::ACTION_LEFT,
            vec![KeyCode::LShift, KeyCode::Left],
        )
        .bind(
            navigation::ACTION_LEFT,
            vec![KeyCode::RShift, KeyCode::Left],
        )
        .bind_with_deadzone(
            navigation::ACTION_LEFT,
            GamepadAxisDirection::LeftStickXNegative,
            0.5,
        )
        .bind(
            navigation::ACTION_RIGHT,
            vec![KeyCode::LShift, KeyCode::Right],
        )
        .bind(
            navigation::ACTION_RIGHT,
            vec![KeyCode::RShift, KeyCode::Right],
        )
        .bind_with_deadzone(
            navigation::ACTION_RIGHT,
            GamepadAxisDirection::LeftStickXPositive,
            0.5,
        )
        .bind(navigation::ACTION_ROTATE_LEFT, KeyCode::Left)
        .bind_with_deadzone(
            navigation::ACTION_ROTATE_LEFT,
            GamepadAxisDirection::RightStickXNegative,
            0.5,
        )
        .bind(navigation::ACTION_ROTATE_RIGHT, KeyCode::Right)
        .bind_with_deadzone(
            navigation::ACTION_ROTATE_RIGHT,
            GamepadAxisDirection::RightStickXPositive,
            0.5,
        )
        .bind(SPEAK_COORDINATES, KeyCode::C)
        .bind(SPEAK_HEADING, KeyCode::H);
    Ok(())
}

// Ugh, and the asset-loading madness continues...
fn load(
    mut state: ResMut<State<AppState>>,
    asset_server: Res<AssetServer>,
    handles: ResMut<AssetHandles>,
    buffers: Res<bevy_openal::Buffers>,
) -> Result<(), Box<dyn Error>> {
    let buffers_created = buffers.0.keys().len();
    let sfx_loaded = asset_server.get_group_load_state(handles.sfx.iter().map(|handle| handle.id))
        == LoadState::Loaded;
    if sfx_loaded && buffers_created == handles.sfx.len() {
        state.overwrite_replace(AppState::InGame)?;
    }
    Ok(())
}

fn spawn_player(mut commands: Commands, sfx: Res<Sfx>) {
    commands
        .spawn()
        .insert_bundle(PlayerBundle {
            ..Default::default()
        })
        .with_children(|parent| {
            parent.spawn().insert_bundle(FootstepBundle {
                footstep: Footstep {
                    sound: sfx.player_footstep,
                    ..Default::default()
                },
                ..Default::default()
            });
        });
}

fn speak_info(
    input: Res<InputMap<String>>,
    mut tts: ResMut<Tts>,
    player: Query<(&Player, &Coordinates, &Yaw)>,
) -> Result<(), Box<dyn Error>> {
    if input.just_active(SPEAK_COORDINATES) {
        if let Ok((_, coordinates, _)) = player.single() {
            tts.speak(
                format!("({}, {})", coordinates.x_i32(), coordinates.y_i32()),
                true,
            )?;
        }
    }
    if input.just_active(SPEAK_HEADING) {
        if let Ok((_, _, yaw)) = player.single() {
            tts.speak(format!("{} degrees", yaw.degrees_u32()), true)?;
        }
    }
    Ok(())
}
