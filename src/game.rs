use std::{
    collections::{HashMap, HashSet},
    error::Error,
    f32::consts::PI,
    time::Duration,
};

use bevy::{
    asset::{HandleId, LoadState},
    prelude::*,
    tasks::AsyncComputeTaskPool,
    utils::Instant,
};
use big_brain::prelude::*;
use blackout::{
    bevy_input_actionmap::{GamepadAxisDirection, InputMap},
    bevy_openal::{efx, Buffer, Buffers, Context, GlobalEffects, Listener, Sound, SoundState},
    bevy_tts::Tts,
    core::{Angle, Area, Coordinates, MovementDirection, Player, PointLike},
    crossbeam_channel::{unbounded, Receiver, Sender},
    derive_more::{Deref, DerefMut},
    error::error_handler,
    exploration::Mappable,
    log::Log,
    map::{Areas, Exit, Map, MapBundle, MapConfig},
    mapgen,
    mapgen::{MapBuilder, TileType},
    navigation,
    navigation::{
        BlocksMotion, Collision, MaxSpeed, MonitorsCollisions, MotionBlocked, NavigationConfig,
        RotationSpeed, Speed, Velocity,
    },
    pathfinding::{find_path, Destination},
    rand::prelude::*,
    sound::{Footstep, FootstepBundle, SoundIcon, SoundIconBundle},
    visibility::{BlocksVisibility, Viewshed, VisibilityBlocked},
};

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
            .add_plugins(DefaultPlugins)
            .add_plugins(blackout::core::CorePlugins)
            .add_plugin(BigBrainPlugin)
            .add_plugin(blackout::bevy_input_actionmap::ActionPlugin::<String>::default())
            .add_plugin(blackout::exploration::ExplorationPlugin)
            .add_plugin(blackout::log::LogPlugin)
            .insert_resource(MapConfig {
                speak_area_descriptions: false,
                start_revealed: true,
                ..Default::default()
            })
            .add_plugin(blackout::map::MapPlugin)
            .add_plugin(blackout::navigation::NavigationPlugin::<AppState>::default())
            .add_plugin(blackout::pathfinding::PathfindingPlugin)
            .add_plugin(blackout::sound::SoundPlugin)
            .add_plugin(blackout::visibility::VisibilityPlugin)
            .add_event::<Reset>()
            .add_event::<RobotKilled>()
            .add_event::<WallCollision>()
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
            .add_system_set(
                SystemSet::on_exit(AppState::Loading).with_system(spawn_player.system()),
            )
            .add_system_set(
                SystemSet::on_exit(AppState::GameOver).with_system(spawn_player.system()),
            )
            .add_system_set(
                SystemSet::on_enter(AppState::InGame)
                    .with_system(send_new_game_event.system())
                    .with_system(setup_level.system()),
            )
            .add_system(
                exit_post_processor
                    .system()
                    .after(HIGHLIGHT_NEXT_EXIT_LABEL),
            )
            .add_system(spawn_robots.system())
            .add_system(sees_player_scorer.system())
            .add_system_to_stage(CoreStage::PreUpdate, pursue_player.system())
            .add_system(taunt_player.system())
            .add_system_to_stage(CoreStage::PreUpdate, investigate_coordinates.system())
            .add_system(curious_scorer.system())
            .add_system_to_stage(CoreStage::PreUpdate, investigate.system())
            .add_system(robot_killed.system())
            .add_system(bonus.system())
            .add_system(bonus_clear.system())
            .add_system(spawn_ambience.system())
            .add_system(spawn_level_exit.system())
            .add_system(position_player_at_start.system())
            .add_system_set(
                SystemSet::on_update(AppState::InGame)
                    .with_system(speak_info.system().chain(error_handler.system()))
                    .with_system(snap.system())
                    .with_system(shoot.system())
                    .with_system(shoot_player.system())
                    .with_system(bullet.system())
                    .with_system(wall_collide.system())
                    .with_system(wall_uncollide.system())
                    .with_system(level_up.system().chain(error_handler.system()))
                    .with_system(shockwave.system()),
            )
            .add_system(
                highlight_next_exit
                    .system()
                    .label(HIGHLIGHT_NEXT_EXIT_LABEL),
            )
            .add_system(next_exit_added.system())
            .add_system_to_stage(CoreStage::PostUpdate, next_exit_removed.system())
            .add_system(checkpoint.system())
            .add_event::<LifeLost>()
            .add_system(life_loss.system().chain(error_handler.system()))
            .add_system_set(
                SystemSet::on_enter(AppState::BetweenLives)
                    .with_system(reset_between_lives_timer.system())
                    .with_system(despawn_player_bullets.system()),
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
                SystemSet::on_enter(AppState::LevelUp).with_system(level_up_enter.system()),
            )
            .add_system_set(
                SystemSet::on_update(AppState::LevelUp)
                    .with_system(level_up_update.system().chain(error_handler.system())),
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
    bonus_clear: HandleId,
    bonus: HandleId,
    bullet: HandleId,
    bullet_wall: HandleId,
    drone: HandleId,
    exit: HandleId,
    exit_correct: HandleId,
    level_exit: HandleId,
    life_lost: HandleId,
    player_footstep: HandleId,
    player_shoot: HandleId,
    robot_badass: HandleId,
    robot_dumbass: HandleId,
    robot_explode: HandleId,
    robot_footstep: HandleId,
    robot_jackass: HandleId,
    robot_shoot: HandleId,
    shockwave: HandleId,
    taunts: Vec<HandleId>,
    wall_power_up: HandleId,
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

#[derive(Clone, Copy, Debug, Deref, DerefMut)]
struct Lives(u32);

impl Default for Lives {
    fn default() -> Self {
        Lives(3)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Checkpoint(Coordinates, Quat);

#[derive(Clone, Debug, Default, Deref, DerefMut)]
struct ShotTimer(Timer);

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
struct ShotRange(u32);

#[derive(Clone, Debug, Default, Deref, DerefMut)]
struct ShotSpeed(u32);

#[derive(Clone, Debug, Deref, DerefMut)]
struct WallCollisionTimer(Timer);

impl Default for WallCollisionTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(1., false))
    }
}

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
            rotation_speed: RotationSpeed(Angle::Degrees(90.)),
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
            shot_timer: ShotTimer(Timer::from_seconds(0.1, false)),
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
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut handles: ResMut<AssetHandles>,
    mut input: ResMut<InputMap<String>>,
    context: ResMut<Context>,
    mut global_effects: ResMut<GlobalEffects>,
) -> Result<(), Box<dyn Error>> {
    commands.spawn().insert(RobotKillTimes::default());
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
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    mut log: Query<&mut Log>,
) {
    if let Ok(mut level) = level.single_mut() {
        **level += 1;
        let map_dimension = 5 + (**level / 2);
        let room_dimension = 16;
        let tile_dimension = (map_dimension * (room_dimension * 2)) as usize;
        let map = MapBuilder::new(tile_dimension, tile_dimension)
            .with(blackout::map::GridBuilder::new(
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
                    gain: 0.2,
                    looping: true,
                    ..Default::default()
                });
            });
        if let Ok(mut log) = log.single_mut() {
            log.push(format!("Level {}.", **level));
        }
    }
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
                gain: 0.4,
                interval: None,
                ..Default::default()
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

#[derive(Clone, Copy, Debug)]
enum RobotType {
    Dumbass,
    Jackass,
    Badass,
}

#[derive(Clone, Copy, Debug)]
struct Robot(RobotType);

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
struct ShotAccuracy(f32);

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
    shot_timer: ShotTimer,
    shot_range: ShotRange,
    shot_speed: ShotSpeed,
    shot_accuracy: ShotAccuracy,
}

fn spawn_robots(
    mut commands: Commands,
    sfx: Res<Sfx>,
    level: Query<&Level>,
    map: Query<(Entity, &Map, &Areas), Added<Areas>>,
    mut log: Query<&mut Log>,
) {
    if let Ok(level) = level.single() {
        if let Ok((entity, map, areas)) = map.single() {
            let base_robots = 20;
            let extra_robots = (**level - 1) * 10;
            let total_robots = base_robots + extra_robots;
            let mut robot_types = vec![RobotType::Dumbass; base_robots as usize];
            match **level {
                2 => {
                    for _ in 0..5 {
                        robot_types.push(RobotType::Dumbass);
                    }
                    for _ in 5..10 {
                        robot_types.push(RobotType::Jackass);
                    }
                }
                v if v > 2 => {
                    for _ in 0..(extra_robots as f32 * 0.3) as u32 {
                        robot_types.push(RobotType::Dumbass);
                    }
                    for _ in 0..(extra_robots as f32 * 0.5) as u32 {
                        robot_types.push(RobotType::Jackass);
                    }
                    for _ in 0..(extra_robots as f32 * 0.2) as u32 {
                        robot_types.push(RobotType::Badass);
                    }
                }
                _ => {}
            };
            if let Some(start) = map.start() {
                let mut rng = thread_rng();
                robot_types.shuffle(&mut rng);
                let starting_area = areas.iter().find(|a| a.contains(&start)).unwrap();
                let areas = areas
                    .iter()
                    .cloned()
                    .filter(|a| a != starting_area)
                    .collect::<Vec<Area>>();
                let mut spawned_robots = 0;
                let mut candidate_areas = areas.clone();
                candidate_areas.shuffle(&mut rng);
                let mut all_robot_coords: Vec<(usize, usize)> = vec![];
                let mut dumbass_count = 0;
                let mut jackass_count = 0;
                let mut badass_count = 0;
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
                    if let Some(robot_type) = robot_types.pop() {
                        let name;
                        let max_speed;
                        let visibility_range;
                        let shot_accuracy;
                        let sound;
                        match robot_type {
                            RobotType::Dumbass => {
                                dumbass_count += 1;
                                name = Name::new(format!("Dumbass {}", dumbass_count));
                                max_speed = MaxSpeed(2.);
                                visibility_range = 12;
                                shot_accuracy = ShotAccuracy(PI / 9.);
                                sound = sfx.robot_dumbass;
                            }
                            RobotType::Jackass => {
                                jackass_count += 1;
                                name = Name::new(format!("Jackass {}", jackass_count));
                                max_speed = MaxSpeed(4.);
                                visibility_range = 16;
                                shot_accuracy = ShotAccuracy(PI / 10.);
                                sound = sfx.robot_jackass;
                            }
                            RobotType::Badass => {
                                badass_count += 1;
                                name = Name::new(format!("Badass {}", badass_count));
                                max_speed = MaxSpeed(4.);
                                visibility_range = 24;
                                shot_accuracy = ShotAccuracy(PI / 12.);
                                sound = sfx.robot_badass;
                            }
                        };
                        let entity_id = commands
                            .spawn()
                            .insert_bundle(RobotBundle {
                                robot: Robot(RobotType::Jackass),
                                coordinates: robot_coords.into(),
                                transform: Default::default(),
                                global_transform: Default::default(),
                                speed: Default::default(),
                                max_speed,
                                velocity: Default::default(),
                                name,
                                viewshed: Viewshed {
                                    range: visibility_range,
                                    ..Default::default()
                                },
                                blocks_visibility: Default::default(),
                                blocks_motion: Default::default(),
                                shot_timer: ShotTimer(Timer::from_seconds(3., false)),
                                shot_range: ShotRange(16),
                                shot_speed: ShotSpeed(8),
                                shot_accuracy,
                            })
                            .insert(
                                Thinker::build()
                                    .picker(FirstToScore { threshold: 0.8 })
                                    .when(SeesPlayer::build(), PursuePlayer::build())
                                    .when(Curious::build(), Investigate::build()),
                            )
                            .with_children(|parent| {
                                let mut timer = Timer::from_seconds(10., false);
                                timer.set_elapsed(Duration::from_secs(10));
                                parent
                                    .spawn()
                                    .insert(Transform::default())
                                    .insert(GlobalTransform::default())
                                    .insert(timer);
                                parent.spawn().insert_bundle(FootstepBundle {
                                    footstep: Footstep {
                                        sound: sfx.robot_footstep,
                                        step_length: 2.,
                                        gain: 1.5,
                                        reference_distance: 10.,
                                        rolloff_factor: 1.5,
                                        pitch_variation: None,
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                });
                                parent.spawn().insert_bundle(SoundIconBundle {
                                    sound_icon: SoundIcon {
                                        sound,
                                        gain: 0.8,
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                });
                            })
                            .id();
                        commands.entity(entity).push_children(&[entity_id]);
                    }
                    spawned_robots += 1;
                }
            }
            if let Ok(mut log) = log.single_mut() {
                let robot_or_robots = if total_robots == 1 { "robot" } else { "robots" };
                log.push(format!("{} {} remaining.", total_robots, robot_or_robots));
            }
        }
    }
}

#[derive(Clone, Debug)]
struct DeathTimer(Timer, Name);

fn robot_killed(
    mut commands: Commands,
    mut events: EventReader<RobotKilled>,
    mut log: Query<&mut Log>,
    names: Query<&Name>,
    level: Query<(Entity, &Map)>,
    transforms: Query<&Transform>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    mut motion_blocked: Query<&mut MotionBlocked>,
    mut visibility_blocked: Query<&mut VisibilityBlocked>,
    coordinates: Query<&Coordinates>,
    non_exploding_robots: Query<(Entity, &Robot, &Coordinates), Without<DeathTimer>>,
    mut killed: Local<HashSet<Entity>>,
) {
    for RobotKilled(entity, _, index, cause) in events.iter() {
        if !killed.contains(&entity) {
            if let Ok(mut log) = log.single_mut() {
                if let Ok(name) = names.get(*entity) {
                    match cause {
                        CauseOfDeath::Bullet(_) => {
                            let mut messages = vec![
                                "is toast!",
                                "is defeated!",
                                "is no more!",
                                "is obliterated!",
                                "exits stage left!",
                                "just suffered a warranty-voiding event!",
                            ];
                            messages.shuffle(&mut thread_rng());
                            let message = format!("{} {}", **name, messages[0]);
                            log.push(message);
                        }
                        CauseOfDeath::Shockwave(owner) => {
                            log.push(format!(
                                "{} is taken out by an exploding {}!",
                                **name, **owner
                            ));
                        }
                    };
                }
            }
            commands.entity(*entity).despawn_recursive();
            if let Ok((level_entity, _)) = level.single() {
                if let Ok(transform) = transforms.get(*entity) {
                    let id = commands
                        .spawn()
                        .insert(Sound {
                            buffer: buffers.get_handle(sfx.robot_explode),
                            state: SoundState::Playing,
                            reference_distance: 10.,
                            ..Default::default()
                        })
                        .insert(*transform)
                        .id();
                    commands.entity(level_entity).push_children(&[id]);
                }
            }
            if let Ok(mut motion_blocked) = motion_blocked.single_mut() {
                motion_blocked[*index] = false;
            }
            if let Ok(mut visibility_blocked) = visibility_blocked.single_mut() {
                visibility_blocked[*index] = false;
            }
            if let Ok(robot_coordinates) = coordinates.get(*entity) {
                for (candidate_entity, _, candidate_coordinates) in non_exploding_robots.iter() {
                    if *entity == candidate_entity {
                        continue;
                    }
                    let distance = robot_coordinates.distance(candidate_coordinates);
                    if distance <= 5. {
                        if let Ok(name) = names.get(*entity) {
                            commands.entity(candidate_entity).insert(DeathTimer(
                                Timer::from_seconds(distance / 2., false),
                                name.clone(),
                            ));
                            let sound = commands
                                .spawn()
                                .insert(Sound {
                                    buffer: buffers.get_handle(sfx.shockwave),
                                    state: SoundState::Playing,
                                    looping: true,
                                    reference_distance: 5.,
                                    ..Default::default()
                                })
                                .insert(Transform::default())
                                .insert(GlobalTransform::default())
                                .id();
                            commands.entity(candidate_entity).push_children(&[sound]);
                        }
                    }
                }
            }
            killed.insert(*entity);
        }
    }
}

fn shockwave(
    time: Res<Time>,
    mut exploding: Query<(Entity, &Coordinates, &mut DeathTimer, &Children)>,
    mut sounds: Query<&mut Sound>,
    level: Query<&Map>,
    mut robot_killed: EventWriter<RobotKilled>,
) {
    for (entity, coordinates, mut timer, children) in exploding.iter_mut() {
        timer.0.tick(time.delta());
        if let Some(sound_entity) = children.last() {
            if let Ok(mut sound) = sounds.get_mut(*sound_entity) {
                sound.pitch = 1. - timer.0.percent() / 2.;
            }
        }
        if timer.0.finished() {
            if let Ok(map) = level.single() {
                let index = coordinates.to_index(map.width());
                robot_killed.send(RobotKilled(
                    entity,
                    *coordinates,
                    index,
                    CauseOfDeath::Shockwave(timer.1.clone()),
                ));
            }
        }
    }
}

#[derive(Clone, Debug, Default, Deref, DerefMut)]
struct RobotKillTimes(Vec<Instant>);

fn bonus(
    mut commands: Commands,
    mut events: EventReader<RobotKilled>,
    mut robot_kill_times: Query<&mut RobotKillTimes>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    level: Query<(&Map, Entity)>,
) {
    for _ in events.iter() {
        if let Ok((_, map_entity)) = level.single() {
            if let Ok(mut robot_kill_times) = robot_kill_times.single_mut() {
                robot_kill_times.push(Instant::now());
                let buffer = buffers.get_handle(sfx.bonus);
                let recent_kills = (robot_kill_times.len() % 7) - 1;
                let notes = vec![0., 2., 4., 5., 7., 9., 11.];
                let pitch = 1. + notes[recent_kills] / 12.;
                let sound_id = commands
                    .spawn()
                    .insert(Sound {
                        buffer,
                        state: SoundState::Playing,
                        gain: 3.,
                        pitch,
                        ..Default::default()
                    })
                    .id();
                commands.entity(map_entity).push_children(&[sound_id]);
            }
        }
    }
}

fn bonus_clear(
    mut commands: Commands,
    mut robot_kill_times: Query<&mut RobotKillTimes>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    level: Query<(&Map, Entity)>,
    mut events: EventReader<Reset>,
) {
    if let Ok(mut robot_kill_times) = robot_kill_times.single_mut() {
        for _ in events.iter() {
            robot_kill_times.clear();
        }
        if robot_kill_times.is_empty() {
            return;
        }
        robot_kill_times.retain(|v| v.elapsed().as_secs() <= 10);
        if robot_kill_times.is_empty() {
            if let Ok((_, map_entity)) = level.single() {
                let buffer = buffers.get_handle(sfx.bonus_clear);
                let sound_id = commands
                    .spawn()
                    .insert(Sound {
                        buffer,
                        state: SoundState::Playing,
                        gain: 3.,
                        ..Default::default()
                    })
                    .id();
                commands.entity(map_entity).push_children(&[sound_id]);
            }
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
                    gain: 0.3,
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
            let direction: MovementDirection = yaw.into();
            tts.speak(format!("{}", direction), true)?;
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
        let robot_or_robots = if robot_count == 1 { "robot" } else { "robots" };
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
            } else if (PI / 2.0..PI).contains(&yaw) {
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

#[derive(Clone, Copy, Debug)]
struct Bullet(Entity);

#[derive(Bundle, Default)]
struct BulletBundle {
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
        Entity,
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
    if let Ok((_, player_entity, coordinates, transform, mut timer, shot_range, shot_speed)) =
        player.single_mut()
    {
        timer.tick(time.delta());
        if input.active(SHOOT) && timer.finished() {
            if let Ok((level_entity, _)) = level.single() {
                let shot_sound = commands
                    .spawn()
                    .insert(Sound {
                        buffer: buffers.get_handle(sfx.player_shoot),
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
                    .insert(Bullet(player_entity))
                    .insert_bundle(BulletBundle {
                        coordinates: *coordinates,
                        range: *shot_range,
                        velocity,
                        sound: Sound {
                            buffer: buffers.get_handle(sfx.bullet),
                            state: SoundState::Playing,
                            looping: true,
                            bypass_global_effects: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .id();
                commands
                    .entity(level_entity)
                    .push_children(&[shot_sound, bullet]);
            }
            timer.reset();
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
) {
    if let Ok((_, player_coords)) = player.single() {
        for (Actor(actor), mut score) in query.iter_mut() {
            if let Ok(viewshed) = viewsheds.get(*actor) {
                if viewshed.is_visible(player_coords) {
                    score.set(1.);
                    continue;
                }
            }
            score.set(0.);
        }
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
    names: Query<&Name>,
    robot: Query<&MaxSpeed>,
) {
    for (Actor(actor), mut state) in query.iter_mut() {
        match *state {
            ActionState::Requested => {
                if let Ok(mut log) = log.single_mut() {
                    if let Ok(name) = names.get(*actor) {
                        log.push(format!("{} is chasing you!", **name));
                    }
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
                    if let Ok(name) = names.get(*actor) {
                        log.push(format!("{} evaded!", **name));
                    }
                }
                *state = ActionState::Success;
            }
            _ => {}
        }
    }
}

fn taunt_player(
    mut commands: Commands,
    query: Query<&Actor, With<PursuePlayer>>,
    time: Res<Time>,
    robots: Query<(&Robot, &Children)>,
    mut timers: Query<&mut Timer>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
) {
    for Actor(actor) in query.iter() {
        if let Ok((_, children)) = robots.get(*actor) {
            let voice = children[0];
            if let Ok(mut timer) = timers.get_mut(voice) {
                timer.tick(time.delta());
                if timer.finished() {
                    let mut taunts = sfx.taunts.clone();
                    taunts.shuffle(&mut thread_rng());
                    let buffer = buffers.get_handle(taunts[0]);
                    let sound = Sound {
                        buffer,
                        state: SoundState::Playing,
                        gain: 1.5,
                        reference_distance: 5.,
                        ..Default::default()
                    };
                    commands.entity(voice).insert(sound);
                    timer.reset();
                }
            }
        }
    }
}

fn shoot_player(
    mut commands: Commands,
    time: Res<Time>,
    query: Query<&Actor, With<PursuePlayer>>,
    mut robots: Query<(
        &Robot,
        Entity,
        &Coordinates,
        &mut ShotTimer,
        &ShotRange,
        &ShotSpeed,
        &ShotAccuracy,
    )>,
    player: Query<(&Player, &Coordinates)>,
    level: Query<(Entity, &Map)>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
) {
    for Actor(actor) in query.iter() {
        if let Ok((_, robot_entity, robot_coords, mut timer, range, speed, accuracy)) =
            robots.get_mut(*actor)
        {
            if let Ok((_, player_coords)) = player.single() {
                timer.tick(time.delta());
                if timer.finished() {
                    if let Ok((level_entity, _)) = level.single() {
                        let transform = Transform::from_translation(Vec3::new(
                            robot_coords.x(),
                            robot_coords.y(),
                            0.,
                        ));
                        let buffer = buffers.get_handle(sfx.robot_shoot);
                        let shot_sound = commands
                            .spawn()
                            .insert(Sound {
                                buffer,
                                state: SoundState::Playing,
                                ..Default::default()
                            })
                            .insert(transform)
                            .id();
                        let bearing = robot_coords.bearing(player_coords);
                        let bearing =
                            thread_rng().gen_range(bearing - **accuracy..bearing + **accuracy);
                        let x = bearing.cos();
                        let y = bearing.sin();
                        let velocity = Vec2::new(x, y) * (**speed as f32);
                        let velocity = Velocity(velocity);
                        let bullet = commands
                            .spawn()
                            .insert(Bullet(robot_entity))
                            .insert_bundle(BulletBundle {
                                coordinates: *robot_coords,
                                range: *range,
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
                        commands
                            .entity(level_entity)
                            .push_children(&[shot_sound, bullet]);
                    }
                    timer.reset();
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deref, DerefMut)]
struct InvestigateCoordinates((i32, i32));

fn investigate_coordinates(
    mut commands: Commands,
    actors: Query<(Entity, &Viewshed, &Coordinates), With<Robot>>,
    bullets: Query<(&Bullet, Entity, &Coordinates)>,
    mut seen_bullets: Local<HashMap<Entity, HashSet<Entity>>>,
    mut robot_kills: EventReader<RobotKilled>,
    mut wall_collisions: EventReader<WallCollision>,
) {
    for (entity, viewshed, _) in actors.iter() {
        if !seen_bullets.contains_key(&entity) {
            seen_bullets.insert(entity, HashSet::new());
        }
        for (_, bullet_entity, bullet_coordinates) in bullets.iter() {
            if let Some(seen_bullets) = seen_bullets.get_mut(&entity) {
                if !seen_bullets.contains(&bullet_entity) && viewshed.is_visible(bullet_coordinates)
                {
                    commands
                        .entity(entity)
                        .insert(InvestigateCoordinates(bullet_coordinates.i32()));
                    seen_bullets.insert(bullet_entity);
                }
            }
        }
    }
    for RobotKilled(_, old_robot_coords, _, _) in robot_kills.iter() {
        for (entity, _, robot_coords) in actors.iter() {
            if robot_coords.distance(old_robot_coords) <= 20. {
                commands
                    .entity(entity)
                    .insert(InvestigateCoordinates(old_robot_coords.i32()));
            }
        }
    }
    for WallCollision(coords) in wall_collisions.iter() {
        for (entity, _, robot_coords) in actors.iter() {
            if robot_coords.distance(coords) <= 30. {
                commands
                    .entity(entity)
                    .insert(InvestigateCoordinates(coords.i32()));
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Curious;

impl Curious {
    fn build() -> CuriousBuilder {
        CuriousBuilder
    }
}

#[derive(Clone, Copy, Debug)]
struct CuriousBuilder;

impl ScorerBuilder for CuriousBuilder {
    fn build(&self, cmd: &mut Commands, scorer: Entity, _actor: Entity) {
        cmd.entity(scorer).insert(Curious);
    }
}

fn curious_scorer(
    mut query: Query<(&Actor, &mut Score), With<Curious>>,
    investigations: Query<&InvestigateCoordinates>,
) {
    for (Actor(actor), mut score) in query.iter_mut() {
        if investigations.get(*actor).is_ok() {
            score.set(0.8);
        } else {
            score.set(0.);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Investigate;

impl Investigate {
    fn build() -> InvestigateBuilder {
        InvestigateBuilder
    }
}

#[derive(Clone, Copy, Debug)]
struct InvestigateBuilder;

impl ActionBuilder for InvestigateBuilder {
    fn build(&self, cmd: &mut Commands, action: Entity, _actor: Entity) {
        cmd.entity(action).insert(Investigate);
    }
}

fn investigate(
    mut commands: Commands,
    mut query: Query<(&Actor, &mut ActionState), With<Investigate>>,
    investigations: Query<&InvestigateCoordinates>,
    max_speeds: Query<&MaxSpeed>,
    destinations: Query<&Destination>,
    coordinates: Query<&Coordinates>,
) {
    for (Actor(actor), mut state) in query.iter_mut() {
        match *state {
            ActionState::Init => {}
            ActionState::Requested => {
                if let Ok(destination) = investigations.get(*actor) {
                    if let Ok(max_speed) = max_speeds.get(*actor) {
                        commands
                            .entity(*actor)
                            .insert(Destination(**destination))
                            .insert(Speed(**max_speed));
                        *state = ActionState::Executing;
                    } else {
                        *state = ActionState::Failure;
                    }
                } else {
                    *state = ActionState::Failure;
                }
            }
            ActionState::Executing => {
                if let Ok(destination) = destinations.get(*actor) {
                    if let Ok(coordinates) = coordinates.get(*actor) {
                        if destination.distance(coordinates) <= 3. {
                            *state = ActionState::Success;
                        }
                    } else {
                        *state = ActionState::Failure;
                    }
                } else {
                    *state = ActionState::Failure;
                }
            }
            ActionState::Cancelled => {
                *state = ActionState::Success;
            }
            _ => {
                commands.entity(*actor).remove::<InvestigateCoordinates>();
            }
        }
    }
}

fn bullet(
    mut commands: Commands,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    mut bullets: Query<(&Bullet, Entity, &Coordinates, &ShotRange, &mut Sound)>,
    mut active_bullets: Local<HashMap<Entity, ((f32, f32), f32)>>,
    state: Res<State<AppState>>,
    robots: Query<(&Robot, Entity, &Coordinates)>,
    level: Query<(Entity, &Map)>,
    mut robot_killed: EventWriter<RobotKilled>,
    player: Query<(&Player, Entity, &Coordinates)>,
    mut log: Query<&mut Log>,
    mut life_lost: EventWriter<LifeLost>,
) {
    for (bullet, entity, coordinates, range, mut sound) in bullets.iter_mut() {
        if !active_bullets.contains_key(&entity) {
            active_bullets.insert(entity, ((coordinates.x(), coordinates.y()), 0.));
        }
        if *state.current() == AppState::BetweenLives {
            println!("Should pause");
            sound.pause();
        } else if sound.state != SoundState::Playing {
            sound.play();
        }
        let mut remove = false;
        if let Ok((map_entity, map)) = level.single() {
            if map.base.at(coordinates.x_usize(), coordinates.y_usize()) == TileType::Wall {
                let transform =
                    Transform::from_translation(Vec3::new(coordinates.x(), coordinates.y(), 0.));
                let zap = commands
                    .spawn()
                    .insert(transform)
                    .insert(Sound {
                        buffer: buffers.get_handle(sfx.bullet_wall),
                        state: SoundState::Playing,
                        gain: 0.8,
                        pitch: (0.9 + random::<f32>() * 0.2),
                        ..Default::default()
                    })
                    .id();
                commands.entity(map_entity).push_children(&[zap]);
                remove = true;
            }
        }
        if let Some((prev_coords, total_distance)) = active_bullets.get_mut(&entity) {
            *total_distance += prev_coords.distance(coordinates);
            if total_distance >= &mut (**range as f32) {
                remove = true;
            }
            let mut ratio = 1. - *total_distance / **range as f32;
            if ratio < 0. {
                ratio = 0.;
            }
            sound.pitch = ratio;
            *prev_coords = (coordinates.x(), coordinates.y());
        }
        let Bullet(owner) = bullet;
        for (_, entity, robot_coordinates) in robots.iter() {
            if *owner != entity && coordinates.distance(robot_coordinates) <= 1. {
                if let Ok((_, map)) = level.single() {
                    let index = robot_coordinates.to_index(map.width());
                    robot_killed.send(RobotKilled(
                        entity,
                        *robot_coordinates,
                        index,
                        CauseOfDeath::Bullet(*owner),
                    ));
                }
                remove = true;
                break;
            }
        }
        if let Ok((_, entity, player_coordinates)) = player.single() {
            if *owner != entity && coordinates.distance(player_coordinates) <= 1. {
                if let Ok(mut log) = log.single_mut() {
                    log.push("Ouch!");
                    life_lost.send(LifeLost);
                }
                remove = true;
            }
        }
        if remove {
            active_bullets.remove(&entity);
            commands.entity(entity).despawn_recursive();
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
                            if step.distance(&coordinates) <= 3. {
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

fn next_exit_added(
    sfx: Res<Sfx>,
    mut next_exit: Query<(&NextExit, &mut SoundIcon), Added<NextExit>>,
) {
    for (_, mut icon) in next_exit.iter_mut() {
        icon.sound = sfx.exit_correct;
    }
}

fn next_exit_removed(
    sfx: Res<Sfx>,
    removed: RemovedComponents<NextExit>,
    mut icons: Query<&mut SoundIcon>,
) {
    for entity in removed.iter() {
        if let Ok(mut icon) = icons.get_component_mut::<SoundIcon>(entity) {
            icon.sound = sfx.exit;
        }
    }
}

fn checkpoint(
    mut player: Query<(&Player, &Coordinates, &Transform, &mut Checkpoint)>,
    mut events: EventReader<Reset>,
    mut cache: Local<Option<Area>>,
    areas: Query<&Areas>,
) {
    for _ in events.iter() {
        *cache = None;
    }
    if let Ok((_, coordinates, transform, mut checkpoint)) = player.single_mut() {
        if let Ok(areas) = areas.single() {
            if let Some(cached_area) = &*cache {
                if checkpoint.0.distance(&coordinates) > 5. {
                    if let Some(current_area) = areas.iter().find(|a| a.contains(coordinates)) {
                        if cached_area != current_area {
                            *cache = Some(current_area.clone());
                            *checkpoint = Checkpoint(*coordinates, transform.rotation);
                        }
                    }
                }
            } else if let Some(current_area) = areas.iter().find(|a| a.contains(coordinates)) {
                *cache = Some(current_area.clone());
                *checkpoint = Checkpoint(*coordinates, transform.rotation);
            }
        }
    }
}

struct LifeLost;

fn life_loss(
    mut commands: Commands,
    mut events: EventReader<LifeLost>,
    mut state: ResMut<State<AppState>>,
    asset_server: Res<AssetServer>,
    sfx: Res<Sfx>,
    mut player: Query<(&Player, &mut Lives)>,
    map: Query<(Entity, &Map)>,
) -> Result<(), Box<dyn Error>> {
    for _ in events.iter() {
        for (_, mut lives) in player.iter_mut() {
            **lives -= 1;
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

fn despawn_player_bullets(
    mut commands: Commands,
    player: Query<Entity, With<Player>>,
    bullets: Query<(Entity, &Bullet)>,
) {
    if let Ok(player_entity) = player.single() {
        for (bullet_entity, bullet) in bullets.iter() {
            let Bullet(owner) = bullet;
            if *owner == player_entity {
                commands.entity(bullet_entity).despawn_recursive();
            }
        }
    }
}

fn tick_between_lives_timer(
    time: Res<Time>,
    mut timer: ResMut<BetweenLivesTimer>,
    mut state: ResMut<State<AppState>>,
    mut player: Query<(
        &Player,
        &Lives,
        &Checkpoint,
        &mut Coordinates,
        &mut Transform,
    )>,
    mut log: Query<&mut Log>,
) -> Result<(), Box<dyn Error>> {
    timer.tick(time.delta());
    if timer.finished() {
        state.pop()?;
        if let Ok((_, lives, checkpoint, mut coordinates, mut transform)) = player.single_mut() {
            if **lives == 0 {
                state.overwrite_replace(AppState::GameOver)?;
            } else {
                let life_or_lives = if **lives > 1 { "lives" } else { "life" };
                if let Ok(mut log) = log.single_mut() {
                    log.push(format!("{} {} left.", **lives, life_or_lives));
                }
                **coordinates = *checkpoint.0;
                transform.rotation = checkpoint.1;
            }
        }
    }
    Ok(())
}

enum CauseOfDeath {
    Bullet(Entity),
    Shockwave(Name),
}

struct RobotKilled(Entity, Coordinates, usize, CauseOfDeath);

struct WallCollision(Coordinates);

fn collision(
    mut commands: Commands,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    mut collisions: EventReader<Collision>,
    player: Query<(Entity, &Player, &Coordinates, Option<&WallCollisionTimer>)>,
    state: Res<State<AppState>>,
    robots: Query<(&Robot, &Name)>,
    mut log: Query<&mut Log>,
    map: Query<(Entity, &Map)>,
    mut life_lost: EventWriter<LifeLost>,
    mut wall_collisions: EventWriter<WallCollision>,
) {
    for event in collisions.iter() {
        for (player_entity, _, player_coordinates, wall_collision_timer) in player.iter() {
            let current_state = state.current();
            if *current_state == AppState::InGame && event.entity == player_entity {
                for (map_entity, map) in map.iter() {
                    if map.base.at(
                        event.coordinates.x() as usize,
                        event.coordinates.y() as usize,
                    ) == TileType::Wall
                    {
                        if wall_collision_timer.is_none() {
                            wall_collisions.send(WallCollision(*player_coordinates));
                            commands
                                .entity(player_entity)
                                .insert(WallCollisionTimer::default());
                            let buffer = buffers.get_handle(sfx.wall_power_up);
                            let sound_id = commands
                                .spawn()
                                .insert(Sound {
                                    buffer,
                                    state: SoundState::Playing,
                                    gain: 0.3,
                                    ..Default::default()
                                })
                                .id();
                            commands.entity(map_entity).push_children(&[sound_id]);
                        }
                    } else if let Ok(mut log) = log.single_mut() {
                        for entity in &map.entities[event.coordinates.to_index(map.width())] {
                            if let Ok((_, name)) = robots.get(*entity) {
                                life_lost.send(LifeLost);
                                log.push(format!("You ran into a very irate {}.", **name));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn wall_collide(
    mut commands: Commands,
    time: Res<Time>,
    mut player: Query<(Entity, &mut WallCollisionTimer, &Lives)>,
    mut log: Query<&mut Log>,
    mut life_lost: EventWriter<LifeLost>,
) {
    for (entity, mut timer, lives) in player.iter_mut() {
        timer.tick(time.delta());
        if timer.finished() {
            commands.entity(entity).remove::<WallCollisionTimer>();
            if **lives > 0 {
                life_lost.send(LifeLost);
            }
            if let Ok(mut log) = log.single_mut() {
                log.push("Wall! Wall! You ran into a wall!");
            }
        }
    }
}

fn wall_uncollide(
    mut commands: Commands,
    player: Query<(Entity, &WallCollisionTimer), Changed<Coordinates>>,
) {
    for (entity, _) in player.iter() {
        commands.entity(entity).remove::<WallCollisionTimer>();
    }
}

fn level_up(
    player: Query<(&Player, &Coordinates, &Viewshed), Changed<Coordinates>>,
    exit: Query<(&LevelExit, &Coordinates)>,
    mut state: ResMut<State<AppState>>,
    robot_coordinates: Query<(&Robot, &Coordinates)>,
) -> Result<(), Box<dyn Error>> {
    for (_, player_coordinates, viewshed) in player.iter() {
        for (_, exit_coordinates) in exit.iter() {
            if player_coordinates.distance(exit_coordinates) < 5. {
                let mut can_advance = true;
                for (_, robot_coordinates) in robot_coordinates.iter() {
                    if viewshed.is_visible(robot_coordinates) {
                        can_advance = false;
                        break;
                    }
                }
                if can_advance {
                    state.push(AppState::LevelUp)?;
                }
            }
        }
    }
    Ok(())
}

fn level_up_enter(level: Query<&Level>, mut lives: Query<&mut Lives>, mut log: Query<&mut Log>) {
    for level in level.iter() {
        if let Ok(mut lives) = lives.single_mut() {
            **lives += 1;
        }
        if let Ok(mut log) = log.single_mut() {
            log.push(format!(
                "Congratulations! You've earned an extra life! Press Enter to continue to level {}.",
                **level + 1
            ));
        }
    }
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

fn game_over_enter(mut commands: Commands, map: Query<(Entity, &Map)>, mut log: Query<&mut Log>) {
    for (entity, _) in map.iter() {
        commands.entity(entity).despawn_recursive();
    }
    if let Ok(mut log) = log.single_mut() {
        log.push("Game over. Press Enter to play again.");
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
