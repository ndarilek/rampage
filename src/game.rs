use std::error::Error;

use bevy::{
    asset::{HandleId, LoadState},
    prelude::*,
};
use blackout::{
    bevy_input_actionmap::{GamepadAxisDirection, InputMap},
    bevy_openal::{efx, Buffers, Context, GlobalEffects},
    core::Player,
    error::error_handler,
    log::Log,
    map::{Map, MapConfig},
    navigation,
    navigation::NavigationConfig,
};

use crate::player::Score;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AppState {
    Loading,
    InGame,
    LevelUp,
    BetweenLives,
    GameOver,
}

// This asset-handling/loading code needs some cleanup.
#[derive(Clone, Debug, Default)]
pub struct AssetHandles {
    gfx: Vec<HandleUntyped>,
    sfx: Vec<HandleUntyped>,
}

#[derive(Clone, Debug)]
pub struct Sprites {
    pub badass: HandleId,
    pub bullet: HandleId,
    pub dumbass: HandleId,
    pub jackass: HandleId,
    pub player: HandleId,
}

impl Default for Sprites {
    fn default() -> Self {
        Self {
            badass: "gfx/red.png".into(),
            bullet: "gfx/silver.png".into(),
            dumbass: "gfx/yellow.png".into(),
            jackass: "gfx/fuchsia.png".into(),
            player: "gfx/blue.png".into(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Sfx {
    pub ambiences: Vec<HandleId>,
    pub bonus_clear: HandleId,
    pub bonus: HandleId,
    pub bullet: HandleId,
    pub bullet_wall: HandleId,
    pub drone: HandleId,
    pub exit: HandleId,
    pub exit_correct: HandleId,
    pub investigate: Vec<HandleId>,
    pub level_exit: HandleId,
    pub life_lost: HandleId,
    pub player_footstep: HandleId,
    pub player_shoot: HandleId,
    pub robot_badass: HandleId,
    pub robot_dumbass: HandleId,
    pub robot_explode: HandleId,
    pub robot_footstep: HandleId,
    pub robot_jackass: HandleId,
    pub robot_shoot: HandleId,
    pub shockwave: HandleId,
    pub taunts: Vec<HandleId>,
    pub wall_power_up: HandleId,
}

impl Default for Sfx {
    fn default() -> Self {
        Self {
            ambiences: vec![
                "sfx/ambience1.flac".into(),
                "sfx/ambience2.flac".into(),
                "sfx/ambience3.flac".into(),
                "sfx/ambience4.flac".into(),
                "sfx/ambience5.flac".into(),
                "sfx/ambience6.flac".into(),
            ],
            bonus_clear: "sfx/bonus_clear.flac".into(),
            bonus: "sfx/bonus.flac".into(),
            bullet: "sfx/bullet.flac".into(),
            bullet_wall: "sfx/bullet_wall.flac".into(),
            drone: "sfx/drone.flac".into(),
            exit: "sfx/exit.flac".into(),
            exit_correct: "sfx/exit_correct.flac".into(),
            investigate: vec![
                "sfx/investigate1.flac".into(),
                "sfx/investigate2.flac".into(),
                "sfx/investigate3.flac".into(),
                "sfx/investigate4.flac".into(),
                "sfx/investigate5.flac".into(),
                "sfx/investigate6.flac".into(),
                "sfx/investigate7.flac".into(),
            ],
            level_exit: "sfx/level_exit.flac".into(),
            life_lost: "sfx/life_lost.flac".into(),
            player_footstep: "sfx/player_footstep.flac".into(),
            player_shoot: "sfx/player_shoot.flac".into(),
            robot_badass: "sfx/robot_badass.flac".into(),
            robot_dumbass: "sfx/robot_dumbass.flac".into(),
            robot_explode: "sfx/robot_explode.flac".into(),
            robot_footstep: "sfx/robot_footstep.flac".into(),
            robot_jackass: "sfx/robot_jackass.flac".into(),
            robot_shoot: "sfx/robot_shoot.flac".into(),
            shockwave: "sfx/shockwave.flac".into(),
            taunts: vec![
                "sfx/taunt1.flac".into(),
                "sfx/taunt2.flac".into(),
                "sfx/taunt3.flac".into(),
                "sfx/taunt4.flac".into(),
                "sfx/taunt5.flac".into(),
                "sfx/taunt6.flac".into(),
                "sfx/taunt7.flac".into(),
                "sfx/taunt8.flac".into(),
            ],
            wall_power_up: "sfx/wall_power_up.flac".into(),
        }
    }
}

pub const SPEAK_COORDINATES: &str = "SPEAK_COORDINATES";
pub const SPEAK_DIRECTION: &str = "SPEAK_DIRECTION";
pub const SPEAK_HEALTH: &str = "SPEAK_HEALTH";
pub const SPEAK_LEVEL: &str = "SPEAK_LEVEL";
pub const SPEAK_ROBOT_COUNT: &str = "SPEAK_ROBOT_COUNT";
pub const SPEAK_SCORE: &str = "SPEAK_SCORE";
pub const SNAP_LEFT: &str = "SNAP_LEFT";
pub const SNAP_RIGHT: &str = "SNAP_RIGHT";
pub const SHOOT: &str = "SHOOT";
pub const CONTINUE: &str = "CONTINUE";

fn setup(
    asset_server: Res<AssetServer>,
    mut handles: ResMut<AssetHandles>,
    mut input: ResMut<InputMap<String>>,
    context: ResMut<Context>,
    mut global_effects: ResMut<GlobalEffects>,
) -> Result<(), Box<dyn Error>> {
    handles.gfx = asset_server.load_folder("gfx")?;
    handles.sfx = asset_server.load_folder("sfx")?;
    let mut slot = context.new_aux_effect_slot()?;
    let mut reverb = context.new_effect::<efx::EaxReverbEffect>()?;
    reverb.set_preset(&efx::REVERB_PRESET_FACTORY_ALCOVE)?;
    reverb.set_preset(&efx::REVERB_PRESET_GENERIC)?;
    slot.set_effect(&reverb)?;
    global_effects.push(slot);
    input
        .bind(navigation::ACTION_FORWARD, KeyCode::Up)
        .bind_with_deadzone(
            navigation::ACTION_FORWARD,
            GamepadAxisDirection::LeftStickYPositive,
            0.5,
        )
        .bind(navigation::ACTION_FORWARD, GamepadButtonType::DPadUp)
        .bind(navigation::ACTION_BACKWARD, KeyCode::Down)
        .bind_with_deadzone(
            navigation::ACTION_BACKWARD,
            GamepadAxisDirection::LeftStickYNegative,
            0.5,
        )
        .bind(navigation::ACTION_BACKWARD, GamepadButtonType::DPadDown)
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
        .bind(navigation::ACTION_ROTATE_LEFT, GamepadButtonType::DPadLeft)
        .bind(navigation::ACTION_ROTATE_RIGHT, KeyCode::Right)
        .bind_with_deadzone(
            navigation::ACTION_ROTATE_RIGHT,
            GamepadAxisDirection::RightStickXPositive,
            0.5,
        )
        .bind(
            navigation::ACTION_ROTATE_RIGHT,
            GamepadButtonType::DPadRight,
        )
        .bind(SPEAK_COORDINATES, KeyCode::C)
        .bind(SPEAK_COORDINATES, GamepadButtonType::LeftThumb)
        .bind(SPEAK_DIRECTION, KeyCode::D)
        .bind(SPEAK_DIRECTION, GamepadButtonType::RightThumb)
        .bind(SPEAK_HEALTH, KeyCode::H)
        .bind(SPEAK_LEVEL, KeyCode::L)
        .bind(SPEAK_ROBOT_COUNT, KeyCode::R)
        .bind(SPEAK_SCORE, KeyCode::S)
        .bind(SNAP_LEFT, vec![KeyCode::LControl, KeyCode::Left])
        .bind(SNAP_LEFT, vec![KeyCode::RControl, KeyCode::Left])
        .bind(SNAP_LEFT, GamepadButtonType::LeftTrigger)
        .bind(SNAP_RIGHT, vec![KeyCode::LControl, KeyCode::Right])
        .bind(SNAP_RIGHT, vec![KeyCode::RControl, KeyCode::Right])
        .bind(SNAP_RIGHT, GamepadButtonType::RightTrigger)
        .bind(SHOOT, KeyCode::Space)
        .bind(SHOOT, GamepadButtonType::LeftTrigger2)
        .bind(SHOOT, GamepadButtonType::RightTrigger2)
        .bind(CONTINUE, KeyCode::Return)
        .bind(CONTINUE, GamepadButtonType::South);
    Ok(())
}

// Ugh, and the asset-loading madness continues...
fn load(
    mut state: ResMut<State<AppState>>,
    asset_server: Res<AssetServer>,
    handles: ResMut<AssetHandles>,
    buffers: Res<Buffers>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) -> Result<(), Box<dyn Error>> {
    let buffers_created = buffers.0.keys().len();
    let gfx_loaded = asset_server.get_group_load_state(handles.gfx.iter().map(|handle| handle.id))
        == LoadState::Loaded;
    let sfx_loaded = asset_server.get_group_load_state(handles.sfx.iter().map(|handle| handle.id))
        == LoadState::Loaded;
    if gfx_loaded && sfx_loaded && buffers_created == handles.sfx.len() {
        let tiles = asset_server.get_handle("sfx/tiles.png");
        materials.add(ColorMaterial::texture(tiles));
        state.overwrite_replace(AppState::InGame)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub enum Reset {
    NewGame,
    NewLevel,
}

fn send_new_game_event(mut events: EventWriter<Reset>) {
    events.send(Reset::NewGame);
}

fn game_over_enter(
    mut commands: Commands,
    map: Query<(Entity, &Map)>,
    score: Query<&Score>,
    mut log: Query<&mut Log>,
) {
    for (entity, _) in map.iter() {
        commands.entity(entity).despawn_recursive();
    }
    if let Ok(score) = score.single() {
        if let Ok(mut log) = log.single_mut() {
            log.push(format!(
                "Game over. Your final score is {}. Press Enter to play again.",
                **score
            ));
        }
    }
}

fn game_over_update(
    mut commands: Commands,
    input: Res<InputMap<String>>,
    mut state: ResMut<State<AppState>>,
    player: Query<(Entity, &Player)>,
    mut events: EventWriter<Reset>,
) -> Result<(), Box<dyn Error>> {
    if input.just_active(CONTINUE) {
        for (entity, _) in player.iter() {
            commands.entity(entity).despawn_recursive();
        }
        state.overwrite_replace(AppState::InGame)?;
        events.send(Reset::NewGame);
    }
    Ok(())
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(blackout::error::ErrorPlugin)
            .insert_resource(WindowDescriptor {
                title: "Rampage".into(),
                ..Default::default()
            })
            .insert_resource(bevy::log::LogSettings {
                level: bevy::log::Level::INFO,
                // filter: "bevy_ecs=trace".into(),
                ..Default::default()
            })
            .insert_resource(NavigationConfig {
                movement_states: vec![AppState::InGame],
                movement_control_states: vec![AppState::InGame],
            })
            .insert_resource(MapConfig {
                speak_area_descriptions: false,
                start_revealed: true,
                ..Default::default()
            })
            .add_plugins(DefaultPlugins)
            .add_system(bevy::input::system::exit_on_esc_system.system())
            .add_plugins(blackout::core::CorePlugins)
            .add_plugin(blackout::bevy_input_actionmap::ActionPlugin::<String>::default())
            .add_plugin(blackout::log::LogPlugin)
            .add_plugin(blackout::map::MapPlugin)
            .add_plugin(blackout::navigation::NavigationPlugin::<AppState>::default())
            .add_plugin(blackout::pathfinding::PathfindingPlugin)
            .add_plugin(blackout::sound::SoundPlugin)
            .add_plugin(blackout::visibility::VisibilityPlugin)
            .add_plugin(crate::ff::ForceFeedbackPlugin)
            .add_plugin(crate::tilemap::TileMapPlugin)
            .add_plugin(crate::player::PlayerPlugin)
            .add_plugin(crate::robot::RobotPlugin)
            .add_plugin(crate::bullet::BulletPlugin)
            .add_plugin(crate::level::LevelPlugin)
            .add_plugin(crate::bonus::BonusPlugin)
            .add_event::<Reset>()
            .add_state(AppState::Loading)
            .init_resource::<AssetHandles>()
            .init_resource::<Sfx>()
            .init_resource::<Sprites>()
            .add_startup_system(setup.system().chain(error_handler.system()))
            .add_system_set(
                SystemSet::on_update(AppState::Loading)
                    .with_system(load.system().chain(error_handler.system())),
            )
            .add_system_set(
                SystemSet::on_enter(AppState::InGame).with_system(send_new_game_event.system()),
            )
            .add_system_set(
                SystemSet::on_enter(AppState::GameOver).with_system(game_over_enter.system()),
            )
            .add_system_set(
                SystemSet::on_update(AppState::GameOver)
                    .with_system(game_over_update.system().chain(error_handler.system())),
            );
    }
}
