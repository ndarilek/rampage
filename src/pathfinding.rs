use std::collections::HashMap;

use bevy::{prelude::*, tasks::prelude::*};
use crossbeam_channel::{unbounded, Receiver};
use derive_more::{Deref, DerefMut};
use pathfinding::prelude::*;

use crate::{
    core::{Coordinates, PointLike},
    map::Map,
    navigation::{MotionBlocked, Speed, Velocity},
};

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut, Eq, Hash, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Destination(pub (i32, i32));

impl_pointlike_for_tuple_component!(Destination);

#[derive(Clone, Debug, Default, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct Path(pub Vec<(i32, i32)>);

fn find_path(
    start: &dyn PointLike,
    destination: &dyn PointLike,
    map: &Map,
) -> Option<(Vec<(i32, i32)>, u32)> {
    astar(
        &start.into(),
        |p| {
            let mut successors: Vec<((i32, i32), u32)> = vec![];
            for tile in map.base.get_available_exits(p.0 as usize, p.1 as usize) {
                successors.push(((tile.0 as i32, tile.1 as i32), (tile.2 * 100.) as u32));
            }
            successors
        },
        |p| (p.distance_squared(destination) * 100.) as u32,
        |p| *p == destination.into(),
    )
}

fn nearest_extreme(from: f32, to: i32) -> f32 {
    let to = to as f32;
    let range = to..=(to + 0.999);
    if from <= *range.start() {
        *range.start()
    } else {
        *range.end()
    }
}

fn cheat_assign(
    start: (f32, f32),
    end: (i32, i32),
    map_width: usize,
    motion_blocked: Vec<bool>,
) -> Option<(f32, f32)> {
    let x;
    let y;
    if start.0 as i32 == end.0 {
        x = start.0;
    } else {
        x = nearest_extreme(start.0, end.0);
    }
    if start.1 as i32 == end.1 {
        y = start.1;
    } else {
        y = nearest_extreme(start.1, end.1);
    }
    let point = (x, y);
    let index = point.to_index(map_width);
    if motion_blocked[index] {
        None
    } else {
        Some(point)
    }
}

fn calculate_path(
    mut commands: Commands,
    pool: Res<AsyncComputeTaskPool>,
    mut calculating: Local<HashMap<Entity, Receiver<Path>>>,
    query: Query<(Entity, &Destination, &Coordinates), Changed<Destination>>,
    destinations: Query<&Destination>,
    map: Query<&Map>,
) {
    let calculating_clone = calculating.clone();
    for (entity, rx) in calculating_clone.iter() {
        if destinations.get(*entity).is_ok() {
            if let Ok(path) = rx.try_recv() {
                commands.entity(*entity).insert(path);
                calculating.remove(&entity);
            }
        } else {
            calculating.remove(&entity);
        }
    }
    for (entity, destination, coordinates) in query.iter() {
        if !calculating.contains_key(&entity) {
            let (tx, rx) = unbounded();
            calculating.insert(entity, rx);
            for map in map.iter() {
                let start_clone = *coordinates;
                let destination_clone = *destination;
                let map_clone = map.clone();
                let tx_clone = tx.clone();
                pool.spawn(async move {
                    if let Some(result) = find_path(&start_clone, &destination_clone, &map_clone) {
                        tx_clone.send(Path(result.0)).expect("Channel should exist");
                    }
                })
                .detach();
            }
        }
    }
}

fn negotiate_path(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Path, &mut Coordinates, &mut Velocity, &Speed)>,
    map: Query<(&Map, &MotionBlocked)>,
) {
    for (entity, mut path, mut coordinates, mut velocity, speed) in query.iter_mut() {
        for (map, motion_blocked) in map.iter() {
            let mut new_path = path.0.clone();
            let start_i32 = (coordinates.x() as i32, coordinates.y() as i32);
            let new_path_clone = new_path.clone();
            let mut iter = new_path_clone.split(|p| *p == start_i32);
            if iter.next().is_some() {
                if let Some(upcoming) = iter.next() {
                    new_path = vec![start_i32];
                    new_path.append(&mut upcoming.to_vec());
                } else {
                    let point = new_path[0];
                    if let Some(new_coords) =
                        cheat_assign(**coordinates, point, map.width(), motion_blocked.0.clone())
                    {
                        **coordinates = new_coords;
                    }
                }
            }
            **path = new_path;
            if path.len() >= 2 {
                let start = **coordinates;
                let start_index = start.to_index(map.width());
                let start = Vec2::new(start.0, start.1);
                let next = path[1];
                let next = Vec2::new(next.0 as f32, next.1 as f32);
                let mut direction = next - start;
                direction = direction.normalize();
                direction *= speed.0;
                let displacement = direction * time.delta_seconds();
                let dest = start + displacement;
                let dest = (dest.x, dest.y);
                let index = dest.to_index(map.width());
                if start_index != index && motion_blocked[index] {
                    let (normal_x, normal_y) = **coordinates;
                    let next = path[1];
                    if let Some((cheat_x, cheat_y)) =
                        cheat_assign(**coordinates, next, map.width(), motion_blocked.0.clone())
                    {
                        let index = (normal_x, cheat_y).to_index(map.width());
                        if !motion_blocked.0[index] {
                            **coordinates = (normal_x, cheat_y);
                            return;
                        }
                        let index = (cheat_x, normal_y).to_index(map.width());
                        if !motion_blocked.0[index] {
                            **coordinates = (cheat_x, normal_y);
                            return;
                        }
                        **coordinates = (cheat_x, cheat_y);
                    }
                    **velocity = Vec2::ZERO;
                } else {
                    **velocity = direction;
                }
            } else {
                commands.entity(entity).remove::<Path>();
                commands.entity(entity).remove::<Destination>();
                **velocity = Vec2::ZERO;
            }
        }
    }
}

pub struct PathfindingPlugin;

impl Plugin for PathfindingPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_system_to_stage(CoreStage::PostUpdate, calculate_path.system())
            .add_system(
                negotiate_path
                    .system()
                    .before(crate::navigation::MOVEMENT_LABEL),
            );
    }
}
