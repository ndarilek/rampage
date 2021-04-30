use std::{error::Error, f32::consts::PI};

use bevy::{prelude::*, tasks::AsyncComputeTaskPool};
use blackout::{
    bevy_input_actionmap::InputMap,
    bevy_openal::{Buffer, Sound, SoundState},
    core::{Area, Coordinates, Player, PointLike},
    crossbeam_channel::{unbounded, Receiver, Sender},
    derive_more::{Deref, DerefMut},
    error::error_handler,
    log::Log,
    map::{Areas, Exit, GridBuilder, Map, MapBundle},
    mapgen,
    mapgen::{MapBuilder, TileType},
    navigation::{Collision, MonitorsCollisions, MotionBlocked},
    pathfinding::find_path,
    rand::prelude::*,
    sound::SoundIcon,
    visibility::{Viewshed, VisibilityBlocked},
};

use crate::{
    game::{AppState, Reset, Sfx, CONTINUE},
    player::{LifeLost, Lives},
    robot::{Robot, RobotCommands, RobotType},
};

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
pub struct Level(u32);

#[derive(Clone, Copy, Debug, Default)]
struct LevelExit;

#[derive(Clone, Copy, Debug, Default)]
struct NextExit;

pub struct WallCollision(pub Coordinates);

#[derive(Clone, Debug, Deref, DerefMut)]
struct WallCollisionTimer(Timer);

impl Default for WallCollisionTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(1., false))
    }
}

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
            .with(GridBuilder::new(
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

fn spawn_robots(
    mut commands: Commands,
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
                        match robot_type {
                            RobotType::Dumbass => {
                                dumbass_count += 1;
                                name = Name::new(format!("Dumbass {}", dumbass_count));
                            }
                            RobotType::Jackass => {
                                jackass_count += 1;
                                name = Name::new(format!("Jackass {}", jackass_count));
                            }
                            RobotType::Badass => {
                                badass_count += 1;
                                name = Name::new(format!("Badass {}", badass_count));
                            }
                        };
                        let coordinates: Coordinates = robot_coords.into();
                        let entity_id = commands
                            .spawn()
                            .insert_robot(&robot_type)
                            .insert(name)
                            .insert(coordinates)
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

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut AppBuilder) {
        const HIGHLIGHT_NEXT_EXIT_LABEL: &str = "HIGHLIGHT_NEXT_EXIT";
        app.add_event::<WallCollision>()
            .add_system_set(SystemSet::on_enter(AppState::InGame).with_system(setup_level.system()))
            .add_system(spawn_ambience.system())
            .add_system(spawn_robots.system())
            .add_system(position_player_at_start.system())
            .add_system(spawn_level_exit.system())
            .add_system(
                exit_post_processor
                    .system()
                    .after(HIGHLIGHT_NEXT_EXIT_LABEL),
            )
            .add_system(
                highlight_next_exit
                    .system()
                    .label(HIGHLIGHT_NEXT_EXIT_LABEL),
            )
            .add_system(next_exit_added.system())
            .add_system_to_stage(CoreStage::PostUpdate, next_exit_removed.system())
            .add_system_to_stage(CoreStage::PostUpdate, collision.system())
            .add_system_set(
                SystemSet::on_update(AppState::InGame)
                    .with_system(wall_collide.system())
                    .with_system(wall_uncollide.system()),
            )
            .add_system_set(
                SystemSet::on_update(AppState::InGame)
                    .with_system(level_up.system().chain(error_handler.system())),
            )
            .add_system_set(
                SystemSet::on_enter(AppState::LevelUp).with_system(level_up_enter.system()),
            )
            .add_system_set(
                SystemSet::on_update(AppState::LevelUp)
                    .with_system(level_up_update.system().chain(error_handler.system())),
            );
    }
}
