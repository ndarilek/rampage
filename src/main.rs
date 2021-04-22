#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
use std::{collections::HashMap, error::Error, f32::consts::PI};

use bevy::{
    asset::{HandleId, LoadState},
    prelude::*,
    tasks::AsyncComputeTaskPool,
};
use bevy_input_actionmap::{GamepadAxisDirection, InputMap};
use bevy_openal::{efx, Buffer, Context, GlobalEffects, Listener, Sound, SoundState};
use bevy_tts::Tts;
use big_brain::prelude::*;
use crossbeam_channel::{unbounded, Receiver, Sender};
use derive_more::{Deref, DerefMut};
use mapgen::{MapBuilder, TileType};
use rand::prelude::*;

#[macro_use]
mod core;
mod error;
mod exploration;
mod log;
mod map;
mod navigation;
mod pathfinding;
mod sound;
mod visibility;

use crate::{
    core::{Angle, Area, Coordinates, Player, PointLike},
    error::error_handler,
    exploration::Mappable,
    log::Log,
    map::{Areas, Exit, Map, MapBundle, MapConfig},
    navigation::{
        BlocksMotion, Collision, MaxSpeed, MonitorsCollisions, MotionBlocked, NavigationConfig,
        RotationSpeed, Speed, Velocity,
    },
    pathfinding::{find_path, Destination},
    sound::{Footstep, FootstepBundle, SoundIcon},
    visibility::{BlocksVisibility, Viewshed, VisibilityBlocked},
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
            level: bevy::log::Level::INFO,
            // filter: "bevy_ecs=trace".into(),
            ..Default::default()
        })
        .insert_resource(NavigationConfig {
            movement_states: vec![AppState::InGame],
            movement_control_states: vec![AppState::InGame],
        })
        .add_plugins(DefaultPlugins)
        .add_plugin(bevy_input_actionmap::ActionPlugin::<String>::default())
        .add_plugin(bevy_openal::OpenAlPlugin)
        .add_plugin(bevy_tts::TtsPlugin)
        .add_plugin(BigBrainPlugin)
        .add_plugin(core::CorePlugin)
        .add_plugin(exploration::ExplorationPlugin)
        .add_plugin(log::LogPlugin)
        .insert_resource(MapConfig {
            start_revealed: true,
            ..Default::default()
        })
        .add_plugin(map::MapPlugin)
        .add_plugin(navigation::NavigationPlugin::<AppState>::default())
        .add_plugin(pathfinding::PathfindingPlugin)
        .add_plugin(sound::SoundPlugin)
        .add_plugin(visibility::VisibilityPlugin)
        .add_event::<Reset>()
        .add_state(AppState::Loading)
        .init_resource::<AssetHandles>()
        .init_resource::<Sfx>()
        .init_resource::<BetweenLivesTimer>()
        .add_system(bevy::input::system::exit_on_esc_system.system())
        .add_startup_system(setup.system().chain(error_handler.system()))
        .add_system_set(
            SystemSet::on_update(AppState::Loading)
                .with_system(load.system().chain(error_handler.system())),
        )
        .add_system_set(SystemSet::on_exit(AppState::Loading).with_system(spawn_player.system()))
        .add_system_set(SystemSet::on_exit(AppState::GameOver).with_system(spawn_player.system()))
        .add_system_set(
            SystemSet::on_enter(AppState::InGame)
                .with_system(send_new_game_event.system())
                .with_system(setup_level.system().chain(error_handler.system())),
        )
        .add_system(
            exit_post_processor
                .system()
                .after(HIGHLIGHT_NEXT_EXIT_LABEL),
        )
        .add_system(spawn_robots.system())
        .add_system(sees_player_scorer.system())
        .add_system(pursue_player.system())
        .add_system(spawn_ambience.system())
        .add_system(spawn_level_exit.system())
        .add_system(position_player_at_start.system())
        .add_system_set(
            SystemSet::on_update(AppState::InGame)
                .with_system(speak_info.system().chain(error_handler.system()))
                .with_system(snap.system())
                .with_system(shoot.system())
                .with_system(bullet.system())
                .with_system(level_up.system().chain(error_handler.system())),
        )
        .add_system(
            highlight_next_exit
                .system()
                .label(HIGHLIGHT_NEXT_EXIT_LABEL),
        )
        .add_system(next_exit_added.system())
        .add_system_to_stage(CoreStage::PostUpdate, next_exit_removed.system())
        .add_system(checkpoint.system())
        .add_system(life_loss.system().chain(error_handler.system()))
        .add_system_set(
            SystemSet::on_enter(AppState::BetweenLives)
                .with_system(reset_between_lives_timer.system()),
        )
        .add_system_set(
            SystemSet::on_update(AppState::BetweenLives).with_system(
                tick_between_lives_timer
                    .system()
                    .chain(error_handler.system()),
            ),
        )
        .add_system_to_stage(CoreStage::PostUpdate, collision.system())
        .add_system_set(
            SystemSet::on_enter(AppState::LevelUp)
                .with_system(level_up_enter.system().chain(error_handler.system())),
        )
        .add_system_set(
            SystemSet::on_update(AppState::LevelUp)
                .with_system(level_up_update.system().chain(error_handler.system())),
        )
        .add_system_set(
            SystemSet::on_enter(AppState::GameOver)
                .with_system(game_over_enter.system().chain(error_handler.system())),
        )
        .add_system_set(
            SystemSet::on_update(AppState::GameOver)
                .with_system(game_over_update.system().chain(error_handler.system())),
        )
        .run();
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum AppState {
    Loading,
    InGame,
    LevelUp,
    BetweenLives,
    GameOver,
}

// This asset-handling/loading code needs some cleanup.
#[derive(Clone, Debug, Default)]
struct AssetHandles {
    sfx: Vec<HandleUntyped>,
}

#[derive(Clone, Debug)]
struct Sfx {
    ambiences: Vec<HandleId>,
    bullet: HandleId,
    bullet_wall: HandleId,
    drone: HandleId,
    exit: HandleId,
    level_exit: HandleId,
    life_lost: HandleId,
    player_footstep: HandleId,
    robot_footstep: HandleId,
    robot1: HandleId,
    robot2: HandleId,
    shoot: HandleId,
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
            bullet: "sfx/bullet.flac".into(),
            bullet_wall: "sfx/bullet_wall.flac".into(),
            drone: "sfx/drone.flac".into(),
            exit: "sfx/exit.wav".into(),
            level_exit: "sfx/level_exit.flac".into(),
            life_lost: "sfx/life_lost.flac".into(),
            player_footstep: "sfx/player_footstep.flac".into(),
            robot_footstep: "sfx/robot_footstep.flac".into(),
            robot1: "sfx/robot1.flac".into(),
            robot2: "sfx/robot2.flac".into(),
            shoot: "sfx/shoot.flac".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deref, DerefMut)]
struct Lives(u32);

impl Default for Lives {
    fn default() -> Self {
        Lives(3)
    }
}

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
struct Checkpoint(Coordinates);

#[derive(Clone, Debug, Default, Deref, DerefMut)]
struct ShotTimer(Timer);

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
struct ShotRange(u32);

#[derive(Clone, Debug, Default, Deref, DerefMut)]
struct ShotSpeed(u32);

#[derive(Bundle)]
struct PlayerBundle {
    player: Player,
    listener: Listener,
    coordinates: Coordinates,
    rotation_speed: RotationSpeed,
    transform: Transform,
    global_transform: GlobalTransform,
    speed: Speed,
    max_speed: MaxSpeed,
    velocity: Velocity,
    name: Name,
    mappable: Mappable,
    viewshed: Viewshed,
    blocks_visibility: BlocksVisibility,
    blocks_motion: BlocksMotion,
    lives: Lives,
    checkpoint: Checkpoint,
    shot_timer: ShotTimer,
    shot_range: ShotRange,
    shot_speed: ShotSpeed,
    level: Level,
}

impl Default for PlayerBundle {
    fn default() -> Self {
        Self {
            player: Default::default(),
            listener: Default::default(),
            coordinates: Default::default(),
            rotation_speed: RotationSpeed(Angle::Degrees(45.)),
            transform: Default::default(),
            global_transform: Default::default(),
            speed: Default::default(),
            max_speed: MaxSpeed(12.),
            velocity: Default::default(),
            name: Name::new("You"),
            mappable: Default::default(),
            viewshed: Viewshed {
                range: 24,
                ..Default::default()
            },
            blocks_visibility: Default::default(),
            blocks_motion: Default::default(),
            lives: Default::default(),
            checkpoint: Default::default(),
            level: Default::default(),
            shot_timer: ShotTimer(Timer::from_seconds(0.15, false)),
            shot_range: ShotRange(24),
            shot_speed: ShotSpeed(36),
        }
    }
}

const SPEAK_COORDINATES: &str = "SPEAK_COORDINATES";
const SPEAK_DIRECTION: &str = "SPEAK_DIRECTION";
const SPEAK_HEALTH: &str = "SPEAK_HEALTH";
const SPEAK_LEVEL: &str = "SPEAK_LEVEL";
const SPEAK_ROBOT_COUNT: &str = "SPEAK_ROBOT_COUNT";
const SNAP_LEFT: &str = "SNAP_LEFT";
const SNAP_RIGHT: &str = "SNAP_RIGHT";
const SHOOT: &str = "SHOOT";
const CONTINUE: &str = "CONTINUE";

fn setup(
    asset_server: Res<AssetServer>,
    mut handles: ResMut<AssetHandles>,
    mut input: ResMut<InputMap<String>>,
    context: ResMut<Context>,
    mut global_effects: ResMut<GlobalEffects>,
) -> Result<(), Box<dyn Error>> {
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
        .bind(SPEAK_DIRECTION, KeyCode::D)
        .bind(SPEAK_HEALTH, KeyCode::H)
        .bind(SPEAK_LEVEL, KeyCode::L)
        .bind(SPEAK_ROBOT_COUNT, KeyCode::R)
        .bind(
            exploration::ACTION_EXPLORE_FORWARD,
            vec![KeyCode::LAlt, KeyCode::Up],
        )
        .bind(
            exploration::ACTION_EXPLORE_FORWARD,
            vec![KeyCode::RAlt, KeyCode::Up],
        )
        .bind_with_deadzone(
            exploration::ACTION_EXPLORE_FORWARD,
            GamepadAxisDirection::RightStickYPositive,
            0.5,
        )
        .bind(
            exploration::ACTION_EXPLORE_BACKWARD,
            vec![KeyCode::LAlt, KeyCode::Down],
        )
        .bind(
            exploration::ACTION_EXPLORE_BACKWARD,
            vec![KeyCode::RAlt, KeyCode::Down],
        )
        .bind_with_deadzone(
            exploration::ACTION_EXPLORE_BACKWARD,
            GamepadAxisDirection::RightStickYNegative,
            0.5,
        )
        .bind(
            exploration::ACTION_EXPLORE_LEFT,
            vec![KeyCode::LAlt, KeyCode::Left],
        )
        .bind(
            exploration::ACTION_EXPLORE_LEFT,
            vec![KeyCode::RAlt, KeyCode::Left],
        )
        .bind_with_deadzone(
            exploration::ACTION_EXPLORE_LEFT,
            GamepadAxisDirection::RightStickXNegative,
            0.5,
        )
        .bind(
            exploration::ACTION_EXPLORE_RIGHT,
            vec![KeyCode::LAlt, KeyCode::Right],
        )
        .bind(
            exploration::ACTION_EXPLORE_RIGHT,
            vec![KeyCode::RAlt, KeyCode::Right],
        )
        .bind_with_deadzone(
            exploration::ACTION_EXPLORE_RIGHT,
            GamepadAxisDirection::RightStickXPositive,
            0.5,
        )
        .bind(
            exploration::ACTION_NAVIGATE_TO_EXPLORED,
            GamepadButtonType::RightThumb,
        )
        .bind(SNAP_LEFT, vec![KeyCode::LControl, KeyCode::Left])
        .bind(SNAP_LEFT, vec![KeyCode::RControl, KeyCode::Left])
        .bind(SNAP_RIGHT, vec![KeyCode::LControl, KeyCode::Right])
        .bind(SNAP_RIGHT, vec![KeyCode::RControl, KeyCode::Right])
        .bind(SHOOT, KeyCode::Space)
        .bind(CONTINUE, KeyCode::Return);
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

#[derive(Clone, Copy, Debug)]
enum Reset {
    NewGame,
    NewLevel,
}

fn send_new_game_event(mut events: EventWriter<Reset>) {
    events.send(Reset::NewGame);
}

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
struct Level(u32);

fn setup_level(
    mut commands: Commands,
    mut level: Query<&mut Level>,
    mut tts: ResMut<Tts>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
) -> Result<(), Box<dyn Error>> {
    if let Ok(mut level) = level.single_mut() {
        **level += 1;
        let map_dimension = 5 + (**level / 2);
        let room_dimension = 16;
        let tile_dimension = (map_dimension * (room_dimension * 2)) as usize;
        let map = MapBuilder::new(tile_dimension, tile_dimension)
            .with(crate::map::GridBuilder::new(
                map_dimension,
                map_dimension,
                room_dimension,
                room_dimension,
            ))
            .with(mapgen::filter::AreaStartingPosition::new(
                mapgen::XStart::LEFT,
                mapgen::YStart::TOP,
            ))
            .with(mapgen::filter::DistantExit::new())
            .build();
        let map = Map::new(map);
        commands
            .spawn()
            .insert_bundle(MapBundle {
                map,
                ..Default::default()
            })
            .with_children(|parent| {
                parent.spawn().insert(Sound {
                    buffer: buffers.get_handle(sfx.drone),
                    state: SoundState::Playing,
                    gain: 0.05,
                    looping: true,
                    ..Default::default()
                });
            });
        tts.speak(format!("Level {}.", **level), false)?;
    }
    Ok(())
}

fn spawn_player(mut commands: Commands, sfx: Res<Sfx>) {
    commands
        .spawn()
        .insert_bundle(PlayerBundle::default())
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

fn exit_post_processor(
    mut commands: Commands,
    sfx: Res<Sfx>,
    mut map: Query<(&mut Map, &mut MotionBlocked, &mut VisibilityBlocked)>,
    exits: Query<(Entity, &Exit, &Coordinates), Added<Exit>>,
) {
    if let Ok((mut map, mut motion_blocked, mut visibility_blocked)) = map.single_mut() {
        for (entity, _, coordinates) in exits.iter() {
            commands.entity(entity).insert(Name::new("Exit"));
            commands.entity(entity).insert(SoundIcon {
                sound: sfx.exit,
                gain: 0.1,
                interval: None,
                pitch: 0.5,
            });
            let x = coordinates.x_i32();
            let y = coordinates.y_i32();
            let exit_half_width = 3;
            for x in (x - exit_half_width)..=(x + exit_half_width) {
                for y in (y - exit_half_width)..=(y + exit_half_width) {
                    map.base.set_tile(x as usize, y as usize, TileType::Floor);
                    let coords: Coordinates = (x, y).into();
                    let index = coords.to_index(map.width());
                    motion_blocked[index] = false;
                    visibility_blocked[index] = false;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
struct Robot;

#[derive(Bundle)]
struct RobotBundle {
    robot: Robot,
    coordinates: Coordinates,
    transform: Transform,
    global_transform: GlobalTransform,
    speed: Speed,
    max_speed: MaxSpeed,
    velocity: Velocity,
    name: Name,
    viewshed: Viewshed,
    blocks_visibility: BlocksVisibility,
    blocks_motion: BlocksMotion,
    sound_icon: SoundIcon,
}

fn spawn_robots(
    mut commands: Commands,
    sfx: Res<Sfx>,
    level: Query<&Level>,
    map: Query<(Entity, &Map, &Areas), Added<Areas>>,
) {
    if let Ok(level) = level.single() {
        if let Ok((entity, map, areas)) = map.single() {
            let total_robots = 10 + **level * 5;
            if let Some(start) = map.start() {
                let starting_area = areas.iter().find(|a| a.contains(&start)).unwrap();
                let areas = areas
                    .iter()
                    .cloned()
                    .filter(|a| a != starting_area)
                    .collect::<Vec<Area>>();
                let mut spawned_robots = 0;
                let mut rng = thread_rng();
                let mut candidate_areas = areas.clone();
                candidate_areas.shuffle(&mut rng);
                let mut all_robot_coords: Vec<(usize, usize)> = vec![];
                while spawned_robots < total_robots {
                    let area = candidate_areas[0].clone();
                    candidate_areas.remove(0);
                    if candidate_areas.is_empty() {
                        candidate_areas = areas.clone();
                        candidate_areas.shuffle(&mut rng);
                    }
                    let mut robot_coords = (
                        rng.gen_range(area.rect.x1..area.rect.x2),
                        rng.gen_range(area.rect.y1..area.rect.y2),
                    );
                    while all_robot_coords.contains(&robot_coords) {
                        robot_coords = (
                            rng.gen_range(area.rect.x1..area.rect.x2),
                            rng.gen_range(area.rect.y1..area.rect.y2),
                        );
                    }
                    all_robot_coords.push(robot_coords);
                    let sound = if rand::random() {
                        sfx.robot1
                    } else {
                        sfx.robot2
                    };
                    let entity_id = commands
                        .spawn()
                        .insert_bundle(RobotBundle {
                            robot: Robot,
                            coordinates: robot_coords.into(),
                            transform: Default::default(),
                            global_transform: Default::default(),
                            speed: Default::default(),
                            max_speed: MaxSpeed(2.),
                            velocity: Default::default(),
                            name: Name::new("Robot"),
                            viewshed: Viewshed {
                                range: 16,
                                ..Default::default()
                            },
                            blocks_visibility: Default::default(),
                            blocks_motion: Default::default(),
                            sound_icon: SoundIcon {
                                sound,
                                gain: 0.1,
                                ..Default::default()
                            },
                        })
                        .insert(
                            Thinker::build()
                                .picker(FirstToScore { threshold: 100. })
                                .when(SeesPlayer::build(), PursuePlayer::build()),
                        )
                        //.insert(Destination((1, 1)))
                        //.insert(Speed(2.))
                        .with_children(|parent| {
                            parent.spawn().insert_bundle(FootstepBundle {
                                footstep: Footstep {
                                    sound: sfx.robot_footstep,
                                    step_length: 2.,
                                    gain: 0.3,
                                    pitch_variation: None,
                                },
                                ..Default::default()
                            });
                        })
                        .id();
                    commands.entity(entity).push_children(&[entity_id]);
                    spawned_robots += 1;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SeesPlayer;

impl SeesPlayer {
    fn build() -> SeesPlayerBuilder {
        SeesPlayerBuilder
    }
}

#[derive(Clone, Copy, Debug)]
struct SeesPlayerBuilder;

impl ScorerBuilder for SeesPlayerBuilder {
    fn build(&self, cmd: &mut Commands, scorer: Entity, _actor: Entity) {
        cmd.entity(scorer).insert(SeesPlayer);
    }
}

fn sees_player_scorer(
    mut query: Query<(&Actor, &mut Score), With<SeesPlayer>>,
    viewsheds: Query<&Viewshed>,
    player: Query<(&Player, &Coordinates)>,
    mut last_player_coords: Local<Option<(i32, i32)>>,
) {
    if let Ok((_, coordinates)) = player.single() {
        let coords = coordinates.i32();
        if last_player_coords.is_none() {
            *last_player_coords = Some(coords);
        }
        if *last_player_coords == Some(coords) {
            return;
        }
        for (Actor(actor), mut score) in query.iter_mut() {
            if let Ok(viewshed) = viewsheds.get(*actor) {
                if viewshed.is_visible(coordinates) {
                    score.set(100.);
                } else {
                    score.set(0.);
                }
            }
        }
        *last_player_coords = Some(coords);
    }
}

#[derive(Clone, Copy, Debug)]
struct PursuePlayer;

impl PursuePlayer {
    fn build() -> PursuePlayerBuilder {
        PursuePlayerBuilder
    }
}

#[derive(Clone, Copy, Debug)]
struct PursuePlayerBuilder;

impl ActionBuilder for PursuePlayerBuilder {
    fn build(&self, cmd: &mut Commands, action: Entity, _actor: Entity) {
        cmd.entity(action).insert(PursuePlayer);
    }
}

fn pursue_player(
    mut commands: Commands,
    mut query: Query<(&Actor, &mut ActionState), With<PursuePlayer>>,
    player: Query<(&Player, &Coordinates)>,
    mut log: Query<&mut Log>,
    robot: Query<&MaxSpeed>,
) {
    for (Actor(actor), mut state) in query.iter_mut() {
        match *state {
            ActionState::Requested => {
                if let Ok(mut log) = log.single_mut() {
                    log.push("A robot is chasing you!");
                }
                *state = ActionState::Executing;
            }
            ActionState::Executing => {
                if let Ok((_, coordinates)) = player.single() {
                    if let Ok(max_speed) = robot.get(*actor) {
                        commands
                            .entity(*actor)
                            .insert(Destination(coordinates.i32()))
                            .insert(Speed(**max_speed));
                    }
                }
            }
            ActionState::Cancelled => {
                if let Ok(mut log) = log.single_mut() {
                    log.push("You've evaded a robot.");
                }
                *state = ActionState::Success;
            }
            _ => {}
        }
    }
}

fn spawn_ambience(
    mut commands: Commands,
    sfx: Res<Sfx>,
    buffers: Res<Assets<Buffer>>,
    map: Query<(Entity, &Map, &Areas), Added<Areas>>,
) {
    if let Ok((entity, _, areas)) = map.single() {
        let mut contains_ambience: Vec<Area> = vec![];
        let mut rng = thread_rng();
        for handle in &sfx.ambiences {
            loop {
                let area_index = rng.gen_range(0..areas.len());
                let area = &areas[area_index];
                if contains_ambience.contains(&area) {
                    continue;
                }
                contains_ambience.push(area.clone());
                let sound = Sound {
                    buffer: buffers.get_handle(*handle),
                    state: SoundState::Playing,
                    looping: true,
                    gain: 0.15,
                    ..Default::default()
                };
                let x = (rng.gen_range(area.rect.x1..area.rect.x2)) as f32;
                let y = (rng.gen_range(area.rect.y1..area.rect.y2)) as f32;
                let ambience = commands
                    .spawn()
                    .insert(sound)
                    .insert(Coordinates((x, y)))
                    .insert(Transform::default())
                    .id();
                commands.entity(entity).push_children(&[ambience]);
                break;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct LevelExit;

fn spawn_level_exit(
    mut commands: Commands,
    sfx: Res<Sfx>,
    map: Query<(Entity, &Map, &Areas), Added<Areas>>,
) {
    for (entity, map, areas) in map.iter() {
        if let Some(exit) = map.exit() {
            if let Some(exit_area) = areas.iter().find(|a| a.contains(&exit)) {
                let sound = SoundIcon {
                    sound: sfx.level_exit,
                    gain: 2.,
                    ..Default::default()
                };
                let center = exit_area.center();
                let center = (center.0 as f32, center.1 as f32);
                let exit_entity = commands
                    .spawn()
                    .insert(sound)
                    .insert(Coordinates(center))
                    .insert(Transform::default())
                    .insert(MonitorsCollisions)
                    .insert(LevelExit)
                    .id();
                commands.entity(entity).push_children(&[exit_entity]);
            }
        }
    }
}

fn position_player_at_start(
    mut player: Query<(&Player, &mut Coordinates, &mut Transform)>,
    map: Query<(&Map, &Areas), Added<Areas>>,
) {
    if let Ok((map, areas)) = map.single() {
        if let Some(start) = map.start() {
            if let Ok((_, mut coordinates, mut transform)) = player.single_mut() {
                for area in areas.iter() {
                    if area.contains(&start) {
                        *coordinates = area.center().into();
                        transform.rotation = Quat::from_rotation_z(PI / 2.);
                    }
                }
            }
        }
    }
}

fn speak_info(
    input: Res<InputMap<String>>,
    mut tts: ResMut<Tts>,
    player: Query<(&Player, &Coordinates, &Transform, &Lives, &Level)>,
    robots: Query<&Robot>,
) -> Result<(), Box<dyn Error>> {
    if input.just_active(SPEAK_COORDINATES) {
        if let Ok((_, coordinates, _, _, _)) = player.single() {
            tts.speak(
                format!("({}, {})", coordinates.x_i32(), coordinates.y_i32()),
                true,
            )?;
        }
    }
    if input.just_active(SPEAK_DIRECTION) {
        if let Ok((_, _, transform, _, _)) = player.single() {
            let forward = transform.local_x();
            let yaw = Angle::Radians(forward.y.atan2(forward.x));
            tts.speak(format!("{} degrees", yaw.degrees_u32()), true)?;
        }
    }
    if input.just_active(SPEAK_HEALTH) {
        if let Ok((_, _, _, lives, _)) = player.single() {
            let life_or_lives = if **lives != 1 { "lives" } else { "life" };
            tts.speak(format!("{} {} left.", **lives, life_or_lives), true)?;
        }
    }
    if input.just_active(SPEAK_LEVEL) {
        if let Ok((_, _, _, _, level)) = player.single() {
            tts.speak(format!("Level {}", **level), true)?;
        }
    }
    if input.just_active(SPEAK_ROBOT_COUNT) {
        let robot_count = robots.iter().len();
        let robot_or_robots = if robot_count == 1 { "RObot" } else { "robots" };
        tts.speak(
            format!("{} {} remaining.", robot_count, robot_or_robots),
            true,
        )?;
    }
    Ok(())
}

fn snap(input: Res<InputMap<String>>, mut transform: Query<(&Player, &mut Transform)>) {
    if input.just_active(SNAP_LEFT) {
        for (_, mut transform) in transform.iter_mut() {
            let forward = transform.local_x();
            let yaw = forward.y.atan2(forward.x);
            if (0. ..PI / 2.).contains(&yaw) {
                transform.rotation = Quat::from_rotation_z(PI / 2.);
            } else if yaw >= PI / 2. && yaw < PI {
                transform.rotation = Quat::from_rotation_z(PI);
            } else if yaw < -PI / 2. {
                transform.rotation = Quat::from_rotation_z(-PI / 2.);
            } else {
                transform.rotation = Quat::from_rotation_z(0.);
            }
        }
    }
    if input.just_active(SNAP_RIGHT) {
        for (_, mut transform) in transform.iter_mut() {
            let forward = transform.local_x();
            let yaw = forward.y.atan2(forward.x);
            if yaw == 0. {
                transform.rotation = Quat::from_rotation_z(-PI / 2.);
                return;
            }
            if yaw > 0. && yaw <= PI / 2. {
                transform.rotation = Quat::from_rotation_z(0.);
            } else if yaw > 0. && yaw <= PI {
                transform.rotation = Quat::from_rotation_z(PI / 2.);
            } else if yaw <= -PI / 2. {
                transform.rotation = Quat::from_rotation_z(-PI);
            } else {
                transform.rotation = Quat::from_rotation_z(-PI / 2.);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Bullet;

#[derive(Bundle, Default)]
struct BulletBundle {
    bullet: Bullet,
    coordinates: Coordinates,
    range: ShotRange,
    velocity: Velocity,
    transform: Transform,
    global_transform: GlobalTransform,
    sound: Sound,
}

fn shoot(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<InputMap<String>>,
    mut player: Query<(
        &Player,
        &Coordinates,
        &Transform,
        &mut ShotTimer,
        &ShotRange,
        &ShotSpeed,
    )>,
    level: Query<(Entity, &Map)>,
    sfx: Res<Sfx>,
    buffers: Res<Assets<Buffer>>,
) {
    if let Ok((_, coordinates, transform, mut timer, shot_range, shot_speed)) = player.single_mut()
    {
        timer.tick(time.delta());
        if input.just_active(SHOOT) && timer.finished() {
            if let Ok((entity, _)) = level.single() {
                let shot_sound = commands
                    .spawn()
                    .insert(Sound {
                        buffer: buffers.get_handle(sfx.shoot),
                        state: SoundState::Playing,
                        gain: 0.5,
                        ..Default::default()
                    })
                    .id();
                let mut velocity = Vec3::new(**shot_speed as f32, 0., 0.);
                velocity = transform.compute_matrix().transform_vector3(velocity);
                let velocity = Velocity(Vec2::new(velocity.x, velocity.y));
                let bullet = commands
                    .spawn()
                    .insert_bundle(BulletBundle {
                        coordinates: *coordinates,
                        range: *shot_range,
                        velocity,
                        sound: Sound {
                            buffer: buffers.get_handle(sfx.bullet),
                            state: SoundState::Playing,
                            gain: 0.4,
                            looping: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .id();
                commands.entity(entity).push_children(&[shot_sound, bullet]);
            }
            timer.reset();
        }
    }
}

fn bullet(
    mut commands: Commands,
    mut bullets: Query<(&Bullet, Entity, &Coordinates, &ShotRange, &mut Sound)>,
    mut active_bullets: Local<HashMap<Entity, ((f32, f32), f32)>>,
) {
    for (_, entity, coordinates, range, mut sound) in bullets.iter_mut() {
        if !active_bullets.contains_key(&entity) {
            active_bullets.insert(entity, ((coordinates.x(), coordinates.y()), 0.));
        }
        let mut remove = false;
        if let Some((prev_coords, total_distance)) = active_bullets.get_mut(&entity) {
            *total_distance += prev_coords.distance(coordinates);
            if total_distance >= &mut (**range as f32) {
                commands.entity(entity).despawn_recursive();
                remove = true;
            }
            let mut ratio = 1. - *total_distance / **range as f32;
            if ratio < 0. {
                ratio = 0.;
            }
            sound.pitch = ratio;
            *prev_coords = (coordinates.x(), coordinates.y());
        }
        if remove {
            active_bullets.remove(&entity);
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct NextExit;

const HIGHLIGHT_NEXT_EXIT_LABEL: &str = "HIGHLIGHT_NEXT_EXIT";

enum NextExitMsg {
    Path(Vec<(i32, i32)>),
    NoPath,
}

fn highlight_next_exit(
    mut commands: Commands,
    mut cache: Local<Option<Area>>,
    mut events: EventReader<Reset>,
    player: Query<(&Player, &Coordinates)>,
    map: Query<(&Areas, &Map)>,
    exits: Query<(Entity, &Exit, &Coordinates)>,
    next_exit: Query<(Entity, &NextExit, &Coordinates)>,
    pool: Res<AsyncComputeTaskPool>,
    mut sender: Local<Option<Sender<NextExitMsg>>>,
    mut receiver: Local<Option<Receiver<NextExitMsg>>>,
) {
    for _ in events.iter() {
        *cache = None;
    }
    if sender.is_none() {
        let (tx, rx) = unbounded();
        *sender = Some(tx);
        *receiver = Some(rx);
    }
    if let Some(receiver) = &*receiver {
        if let Ok(msg) = receiver.try_recv() {
            use NextExitMsg::*;
            match msg {
                Path(path) => {
                    for (entity, _, _) in next_exit.iter() {
                        commands.entity(entity).remove::<NextExit>();
                    }
                    for step in path {
                        let step: Coordinates = step.into();
                        for (entity, _, coordinates) in exits.iter() {
                            if step.distance(&coordinates) <= 4. {
                                commands.entity(entity).insert(NextExit);
                                return;
                            }
                        }
                    }
                }
                NoPath => {
                    for (entity, _, _) in next_exit.iter() {
                        commands.entity(entity).remove::<NextExit>();
                    }
                }
            }
        }
    }
    if let Ok((_, coordinates)) = player.single() {
        if let Ok((areas, map)) = map.single() {
            if let Some(current_area) = areas.iter().find(|a| a.contains(coordinates)) {
                let recalculate;
                if let Some(cached_area) = &*cache {
                    if current_area == cached_area {
                        return;
                    } else {
                        *cache = Some(current_area.clone());
                        recalculate = true;
                    }
                } else {
                    *cache = Some(current_area.clone());
                    recalculate = true;
                }
                if recalculate {
                    let start = current_area.center();
                    let map_clone = map.clone();
                    if let Some(sender) = sender.clone() {
                        pool.spawn(async move {
                            if let Some(destination) = map_clone.exit() {
                                if let Some(result) = find_path(&start, &destination, &map_clone) {
                                    let path = result.0;
                                    sender.send(NextExitMsg::Path(path)).unwrap();
                                } else {
                                    sender.send(NextExitMsg::NoPath).unwrap();
                                }
                            }
                        })
                        .detach();
                    }
                }
            }
        }
    }
}

fn next_exit_added(mut next_exit: Query<(&NextExit, &mut SoundIcon), Added<NextExit>>) {
    for (_, mut icon) in next_exit.iter_mut() {
        icon.gain = 0.3;
        icon.pitch = 1.;
    }
}

fn next_exit_removed(removed: RemovedComponents<NextExit>, mut icons: Query<&mut SoundIcon>) {
    for entity in removed.iter() {
        if let Ok(mut icon) = icons.get_component_mut::<SoundIcon>(entity) {
            icon.gain = 0.1;
            icon.pitch = 0.5;
        }
    }
}

fn checkpoint(
    mut player: Query<(&Player, &Coordinates, &mut Checkpoint)>,
    mut events: EventReader<Reset>,
    mut cache: Local<Option<Area>>,
    areas: Query<&Areas>,
) {
    for _ in events.iter() {
        *cache = None;
    }
    if let Ok((_, coordinates, mut checkpoint)) = player.single_mut() {
        if let Ok(areas) = areas.single() {
            if let Some(cached_area) = &*cache {
                if checkpoint.distance(&coordinates) > 5. {
                    if let Some(current_area) = areas.iter().find(|a| a.contains(coordinates)) {
                        if cached_area != current_area {
                            *cache = Some(current_area.clone());
                            **checkpoint = *coordinates;
                        }
                    }
                }
            } else if let Some(current_area) = areas.iter().find(|a| a.contains(coordinates)) {
                *cache = Some(current_area.clone());
                **checkpoint = *coordinates;
            }
        }
    }
}

fn life_loss(
    mut commands: Commands,
    mut state: ResMut<State<AppState>>,
    asset_server: Res<AssetServer>,
    sfx: Res<Sfx>,
    mut player: Query<(&Player, &Lives), Changed<Lives>>,
    map: Query<(Entity, &Map)>,
) -> Result<(), Box<dyn Error>> {
    for (_, lives) in player.iter_mut() {
        if **lives == 3 {
            return Ok(());
        }
        let buffer = asset_server.get_handle(sfx.life_lost);
        let entity_id = commands
            .spawn()
            .insert(Sound {
                buffer,
                state: SoundState::Playing,
                ..Default::default()
            })
            .id();
        if let Ok((entity, _)) = map.single() {
            commands.entity(entity).push_children(&[entity_id]);
        }
        state.push(AppState::BetweenLives)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Deref, DerefMut)]
struct BetweenLivesTimer(Timer);

impl Default for BetweenLivesTimer {
    fn default() -> Self {
        BetweenLivesTimer(Timer::from_seconds(5., false))
    }
}

fn reset_between_lives_timer(mut timer: ResMut<BetweenLivesTimer>) {
    timer.reset();
}

fn tick_between_lives_timer(
    time: Res<Time>,
    mut timer: ResMut<BetweenLivesTimer>,
    mut tts: ResMut<Tts>,
    mut state: ResMut<State<AppState>>,
    mut player: Query<(&Player, &Lives, &Checkpoint, &mut Coordinates)>,
) -> Result<(), Box<dyn Error>> {
    timer.tick(time.delta());
    if timer.finished() {
        state.pop()?;
        if let Ok((_, lives, checkpoint, mut coordinates)) = player.single_mut() {
            if **lives == 0 {
                state.overwrite_replace(AppState::GameOver)?;
            } else {
                let life_or_lives = if **lives > 1 { "lives" } else { "life" };
                tts.speak(format!("{} {} left.", **lives, life_or_lives), true)?;
                **coordinates = ***checkpoint;
            }
        }
    }
    Ok(())
}

fn collision(
    mut commands: Commands,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    mut collisions: EventReader<Collision>,
    bullets: Query<&Bullet>,
    mut player: Query<(Entity, &Player, &mut Lives)>,
    state: Res<State<AppState>>,
    mut log: Query<&mut Log>,
    map: Query<(Entity, &Map)>,
) {
    for event in collisions.iter() {
        if bullets.get(event.entity).is_ok() {
            if let Ok((entity, map)) = map.single() {
                if map.base.at(
                    event.coordinates.x() as usize,
                    event.coordinates.y() as usize,
                ) == TileType::Wall
                {
                    let transform = Transform::from_translation(Vec3::new(
                        event.coordinates.x(),
                        event.coordinates.y(),
                        0.,
                    ));
                    let zap = commands
                        .spawn()
                        .insert(transform)
                        .insert(Sound {
                            buffer: buffers.get_handle(sfx.bullet_wall),
                            state: SoundState::Playing,
                            gain: 0.8,
                            ..Default::default()
                        })
                        .id();
                    commands.entity(entity).push_children(&[zap]);
                }
            }
            commands.entity(event.entity).despawn_recursive();
        }
        for (player_entity, _, mut lives) in player.iter_mut() {
            let current_state = state.current();
            if *current_state == AppState::InGame {
                if event.entity == player_entity {
                    if **lives > 0 {
                        **lives -= 1;
                    }
                    if let Ok(mut log) = log.single_mut() {
                        if let Ok((_, map)) = map.single() {
                            if map.base.at(
                                event.coordinates.x() as usize,
                                event.coordinates.y() as usize,
                            ) == TileType::Wall
                            {
                                log.push("Wall! Wall! You ran into a wall!");
                            } else {
                                log.push("You ran into a very irate robot.");
                            }
                        }
                    }
                }
            }
        }
    }
}

fn level_up(
    player: Query<(&Player, &Coordinates), Changed<Coordinates>>,
    exit: Query<(&LevelExit, &Coordinates)>,
    mut state: ResMut<State<AppState>>,
) -> Result<(), Box<dyn Error>> {
    for (_, player_coordinates) in player.iter() {
        for (_, exit_coordinates) in exit.iter() {
            if player_coordinates.distance(exit_coordinates) < 5. {
                state.push(AppState::LevelUp)?;
            }
        }
    }
    Ok(())
}

fn level_up_enter(mut tts: ResMut<Tts>, level: Query<&Level>) -> Result<(), Box<dyn Error>> {
    for level in level.iter() {
        tts.speak(
            format!(
                "Congratulations! Press Enter to continue to level {}.",
                **level + 1
            ),
            true,
        )?;
    }
    Ok(())
}

fn level_up_update(
    mut commands: Commands,
    input: Res<InputMap<String>>,
    map: Query<(Entity, &Map)>,
    mut events: EventWriter<Reset>,
    mut state: ResMut<State<AppState>>,
) -> Result<(), Box<dyn Error>> {
    if input.just_active(CONTINUE) {
        for (entity, _) in map.iter() {
            commands.entity(entity).despawn_recursive();
        }
        events.send(Reset::NewLevel);
        state.overwrite_replace(AppState::InGame)?;
    }
    Ok(())
}

fn game_over_enter(
    mut commands: Commands,
    mut tts: ResMut<Tts>,
    map: Query<(Entity, &Map)>,
) -> Result<(), Box<dyn Error>> {
    for (entity, _) in map.iter() {
        commands.entity(entity).despawn_recursive();
    }
    tts.speak("Game over. Press Enter to play again.", true)?;
    Ok(())
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
