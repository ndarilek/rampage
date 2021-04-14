#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
use std::{error::Error, f32::consts::PI};

use bevy::{
    asset::{HandleId, LoadState},
    prelude::*,
    tasks::AsyncComputeTaskPool,
};
use bevy_input_actionmap::{GamepadAxisDirection, InputMap};
use bevy_openal::{efx, Buffer, Context, GlobalEffects, Listener, Sound, SoundState};
use bevy_tts::Tts;
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
    map::{Areas, Exit, Map, MapConfig},
    navigation::{
        Collision, MaxSpeed, MotionBlocked, NavigationConfig, RotationSpeed, Speed, Velocity,
    },
    pathfinding::find_path,
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
        .add_state(AppState::Loading)
        .init_resource::<AssetHandles>()
        .init_resource::<Sfx>()
        .init_resource::<Level>()
        .init_resource::<MapDimension>()
        .init_resource::<RoomDimension>()
        .add_system(bevy::input::system::exit_on_esc_system.system())
        .add_startup_system(setup.system().chain(error_handler.system()))
        .add_system_set(
            SystemSet::on_update(AppState::Loading)
                .with_system(load.system().chain(error_handler.system())),
        )
        .add_system_set(
            SystemSet::on_enter(AppState::InGame)
                .with_system(setup_level.system().chain(error_handler.system()))
                .with_system(spawn_player.system()),
        )
        .add_system(
            exit_post_processor
                .system()
                .after(HIGHLIGHT_NEXT_EXIT_LABEL),
        )
        .add_system(spawn_ambience.system())
        .add_system(spawn_level_exit.system())
        .add_system(position_player_at_start.system())
        .add_system_set(
            SystemSet::on_update(AppState::InGame)
                .with_system(speak_info.system().chain(error_handler.system()))
                .with_system(snap.system()),
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
        .add_system_to_stage(CoreStage::PostUpdate, collision.system())
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

#[derive(Clone, Debug)]
struct Sfx {
    ambiences: Vec<HandleId>,
    exit: HandleId,
    level_exit: HandleId,
    player_footstep: HandleId,
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
            exit: "sfx/exit.wav".into(),
            level_exit: "sfx/level_exit.flac".into(),
            player_footstep: "sfx/player_footstep.flac".into(),
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
    lives: Lives,
    checkpoint: Checkpoint,
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
            lives: Default::default(),
            checkpoint: Default::default(),
        }
    }
}

const SPEAK_COORDINATES: &str = "SPEAK_COORDINATES";
const SPEAK_HEADING: &str = "SPEAK_HEADING";
const SNAP_LEFT: &str = "SNAP_LEFT";
const SNAP_RIGHT: &str = "SNAP_RIGHT";

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
        .bind(SPEAK_HEADING, KeyCode::H)
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
        .bind(exploration::ACTION_NAVIGATE_TO_EXPLORED, KeyCode::Return)
        .bind(
            exploration::ACTION_NAVIGATE_TO_EXPLORED,
            GamepadButtonType::RightThumb,
        )
        .bind(SNAP_LEFT, vec![KeyCode::LControl, KeyCode::Left])
        .bind(SNAP_LEFT, vec![KeyCode::RControl, KeyCode::Left])
        .bind(SNAP_RIGHT, vec![KeyCode::LControl, KeyCode::Right])
        .bind(SNAP_RIGHT, vec![KeyCode::RControl, KeyCode::Right]);
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

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
struct Level(u32);

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
struct MapDimension(u32);

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
struct RoomDimension(u32);

fn setup_level(
    mut commands: Commands,
    mut level: ResMut<Level>,
    mut map_dimension: ResMut<MapDimension>,
    mut room_dimension: ResMut<RoomDimension>,
    mut tts: ResMut<Tts>,
) -> Result<(), Box<dyn Error>> {
    **level += 1;
    **map_dimension = 5;
    **room_dimension = 16;
    let map = MapBuilder::new(137, 137)
        .with(crate::map::GridBuilder::new(
            **map_dimension,
            **map_dimension,
            **room_dimension,
            **room_dimension,
        ))
        .with(mapgen::filter::AreaStartingPosition::new(
            mapgen::XStart::LEFT,
            mapgen::YStart::TOP,
        ))
        .with(mapgen::filter::DistantExit::new())
        .build();
    let map = Map::new(map);
    commands.spawn().insert(map).insert(Children::default());
    tts.speak(format!("Level {}.", **level), false)?;
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

fn spawn_level_exit(mut commands: Commands, sfx: Res<Sfx>, map: Query<&Map, Added<Map>>) {
    for map in map.iter() {
        if let Some(exit) = map.exit() {
            let sound = SoundIcon {
                sound: sfx.level_exit,
                gain: 2.,
                ..Default::default()
            };
            commands
                .spawn()
                .insert(sound)
                .insert(Coordinates((exit.x as f32, exit.y as f32)))
                .insert(Transform::default());
            println!("{:?}", exit);
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
    player: Query<(&Player, &Coordinates, &Transform)>,
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
        if input.just_active(SNAP_RIGHT) {}
        if let Ok((_, _, transform)) = player.single() {
            let forward = transform.local_x();
            let yaw = Angle::Radians(forward.y.atan2(forward.x));
            tts.speak(format!("{} degrees", yaw.degrees_u32()), true)?;
        }
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
struct NextExit;

const HIGHLIGHT_NEXT_EXIT_LABEL: &str = "HIGHLIGHT_NEXT_EXIT";

enum NextExitMsg {
    Path(Vec<(i32, i32)>),
    NoPath,
}

fn highlight_next_exit(
    mut commands: Commands,
    mut cache: Local<Option<Area>>,
    player: Query<(&Player, &Coordinates)>,
    map: Query<(&Areas, &Map)>,
    exits: Query<(Entity, &Exit, &Coordinates)>,
    next_exit: Query<(Entity, &NextExit, &Coordinates)>,
    pool: Res<AsyncComputeTaskPool>,
    mut sender: Local<Option<Sender<NextExitMsg>>>,
    mut receiver: Local<Option<Receiver<NextExitMsg>>>,
) {
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
    mut cache: Local<Option<Area>>,
    areas: Query<&Areas>,
) {
    if let Ok((_, coordinates, mut checkpoint)) = player.single_mut() {
        let areas = areas.single().unwrap();
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

fn life_loss(
    mut state: ResMut<State<AppState>>,
    mut tts: ResMut<Tts>,
    mut player: Query<(&Player, &Lives, &Checkpoint, &mut Coordinates), Changed<Lives>>,
) -> Result<(), Box<dyn Error>> {
    for (_, lives, checkpoint, mut coordinates) in player.iter_mut() {
        if **lives == 3 {
            return Ok(());
        }
        if **lives == 0 {
            state.overwrite_replace(AppState::GameOver)?;
            tts.speak("Game over.", true)?;
        } else {
            let life_or_lives = if **lives > 1 { "lives" } else { "life" };
            tts.speak(format!("{} {} left.", **lives, life_or_lives), true)?;
        }
        **coordinates = ***checkpoint;
    }
    Ok(())
}

fn collision(
    mut collisions: EventReader<Collision>,
    mut player: Query<(Entity, &Player, &mut Lives)>,
) {
    for event in collisions.iter() {
        for (player_entity, _, mut lives) in player.iter_mut() {
            if event.entity == player_entity && **lives > 0 {
                **lives -= 1;
            }
        }
    }
}
