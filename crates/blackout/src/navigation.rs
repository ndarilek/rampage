use std::{collections::HashMap, error::Error, fmt::Debug, hash::Hash};

use bevy::prelude::*;
use bevy_input_actionmap::InputMap;
use bevy_tts::Tts;
use derive_more::{Deref, DerefMut};

use crate::{
    core::{Angle, CardinalDirection, Coordinates, Player, PointLike},
    error::error_handler,
    exploration::{ExplorationFocused, Exploring},
    map::{ITileType, Map},
    pathfinding::Destination,
};

#[derive(Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct BlocksMotion;

#[derive(Clone, Debug, Default, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct CollisionsMonitored(pub Vec<bool>);

#[derive(Clone, Copy, Debug, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct MaxSpeed(pub f32);

impl Default for MaxSpeed {
    fn default() -> Self {
        MaxSpeed(2.)
    }
}

#[derive(Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct MonitorsCollisions;

#[derive(Clone, Debug, Default, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct MotionBlocked(pub Vec<bool>);

#[derive(Clone, Copy, Debug, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct RotationSpeed(pub Angle);

impl Default for RotationSpeed {
    fn default() -> Self {
        Self(Angle::Radians(0.))
    }
}

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct Speed(pub f32);

#[derive(Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Sprinting;

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct Velocity(pub Vec2);

#[derive(Clone, Copy, Debug)]
pub struct Collision {
    pub entity: Entity,
    pub coordinates: (f32, f32),
    pub index: usize,
}

pub const ACTION_FORWARD: &str = "forward";
pub const ACTION_BACKWARD: &str = "backward";
pub const ACTION_LEFT: &str = "left";
pub const ACTION_RIGHT: &str = "right";
pub const ACTION_ROTATE_LEFT: &str = "ROTATE_LEFT";
pub const ACTION_ROTATE_RIGHT: &str = "ROTATE_RIGHT";
pub const ACTION_SPRINT: &str = "SPRINT";

fn movement_controls(
    mut commands: Commands,
    input: Res<InputMap<String>>,
    time: Res<Time>,
    mut query: Query<(
        Entity,
        &Player,
        &mut Velocity,
        &mut Speed,
        &MaxSpeed,
        Option<&RotationSpeed>,
        &mut Transform,
        Option<&Destination>,
    )>,
    exploration_focused: Query<(Entity, &ExplorationFocused)>,
) {
    for (
        entity,
        _,
        mut velocity,
        mut speed,
        max_speed,
        rotation_speed,
        mut transform,
        destination,
    ) in query.iter_mut()
    {
        let sprinting = input.active(ACTION_SPRINT);
        if sprinting {
            commands.entity(entity).insert(Sprinting::default());
        } else {
            commands.entity(entity).remove::<Sprinting>();
        }
        let mut direction = Vec3::default();
        if input.active(ACTION_FORWARD) {
            direction.x += 1.;
        }
        if input.active(ACTION_BACKWARD) {
            direction.x -= 1.;
        }
        if input.active(ACTION_LEFT) {
            direction.y += 1.;
        }
        if input.active(ACTION_RIGHT) {
            direction.y -= 1.;
        }
        if let Some(rotation_speed) = rotation_speed {
            let delta = rotation_speed.radians() * time.delta_seconds();
            if input.active(ACTION_ROTATE_LEFT) {
                transform.rotate(Quat::from_rotation_z(delta));
            }
            if input.active(ACTION_ROTATE_RIGHT) {
                transform.rotate(Quat::from_rotation_z(-delta));
            }
        }
        if direction.length_squared() != 0. {
            direction = direction.normalize();
            let forward_x = input.strength(ACTION_FORWARD).abs();
            let backward_x = input.strength(ACTION_BACKWARD).abs();
            let x = if forward_x > backward_x {
                forward_x
            } else {
                backward_x
            };
            let right_y = input.strength(ACTION_RIGHT).abs();
            let left_y = input.strength(ACTION_LEFT).abs();
            let y = if right_y > left_y { right_y } else { left_y };
            let strength = Vec3::new(x, y, 0.);
            let s = if sprinting {
                **max_speed
            } else {
                **max_speed / 3.
            };
            speed.0 = s;
            direction *= s;
            direction *= strength;
            commands.entity(entity).remove::<Destination>();
            commands.entity(entity).remove::<Exploring>();
            for (entity, _) in exploration_focused.iter() {
                commands.entity(entity).remove::<ExplorationFocused>();
            }
            direction = transform.compute_matrix().transform_vector3(direction);
            let direction = Vec2::new(direction.x, direction.y);
            **velocity = direction;
        } else if destination.is_none() {
            **velocity = Vec2::ZERO;
            speed.0 = 0.;
        } else if sprinting {
            speed.0 = max_speed.0;
        } else {
            speed.0 = max_speed.0 / 3.;
        }
    }
}

fn movement(
    time: Res<Time>,
    mut collision_events: EventWriter<Collision>,
    map: Query<(&Map, &MotionBlocked, &CollisionsMonitored)>,
    mut entities: Query<(Entity, &Velocity, &mut Coordinates, Option<&BlocksMotion>)>,
) {
    for (entity, velocity, mut coordinates, blocks_motion) in entities.iter_mut() {
        if **velocity != Vec2::ZERO {
            let displacement = **velocity * time.delta_seconds();
            let mut point = **coordinates;
            point.0 += displacement.x;
            point.1 += displacement.y;
            if let Ok((map, motion_blocked, collisions_monitored)) = map.single() {
                let idx = point.to_index(map.width());
                if idx < map.base.tiles.len() {
                    let current_entities = &map.entities[idx];
                    if blocks_motion.is_some()
                        && motion_blocked[idx]
                        && !current_entities.contains(&entity)
                    {
                        collision_events.send(Collision {
                            entity,
                            coordinates: point,
                            index: idx,
                        });
                    } else {
                        **coordinates = point;
                        let current_entities = &map.entities[idx];
                        if collisions_monitored[idx] && !current_entities.contains(&entity) {
                            collision_events.send(Collision {
                                entity,
                                coordinates: point,
                                index: idx,
                            });
                        }
                    }
                }
            } else {
                **coordinates = point;
            }
        }
    }
}

pub const UPDATE_COLLISION_INDEX_LABEL: &str = "UPDATE_COLLISION_INDEX";

#[derive(Default, Deref, DerefMut)]
struct PreviousBlocksMotionIndex(HashMap<Entity, usize>);

fn blocks_motion_indexing(
    mut map: Query<(&Map, &mut MotionBlocked)>,
    mut prev_index: ResMut<PreviousBlocksMotionIndex>,
    query: Query<
        (Entity, &Coordinates, &BlocksMotion),
        Or<(Changed<Coordinates>, Changed<BlocksMotion>)>,
    >,
    motion_blockers: Query<&BlocksMotion>,
) {
    for (entity, coordinates, _) in query.iter() {
        for (map, mut motion_blocked) in map.iter_mut() {
            let idx = coordinates.to_index(map.width());
            if let Some(prev_idx) = prev_index.get(&entity) {
                if *prev_idx == idx {
                    continue;
                }
                let tile = map.base.tiles[*prev_idx];
                let mut new_motion_blocked = tile.blocks_motion();
                if !new_motion_blocked {
                    for e in &map.entities[*prev_idx] {
                        if motion_blockers.get(*e).is_ok() {
                            new_motion_blocked = true;
                            break;
                        }
                    }
                }
                motion_blocked[*prev_idx] = new_motion_blocked;
            }
            motion_blocked[idx] = true;
            prev_index.insert(entity, idx);
        }
    }
}

fn remove_blocks_motion(
    mut prev_index: ResMut<PreviousBlocksMotionIndex>,
    mut map: Query<(&Map, &mut MotionBlocked)>,
    removed: RemovedComponents<BlocksMotion>,
    coordinates: Query<&Coordinates>,
    blocks_motion: Query<&BlocksMotion>,
) {
    for entity in removed.iter() {
        if let Ok(coordinates) = coordinates.get_component::<Coordinates>(entity) {
            prev_index.remove(&entity);
            for (map, mut motion_blocked) in map.iter_mut() {
                let idx = (**coordinates).to_index(map.width());
                let tile = map.base.tiles[idx];
                let mut new_motion_blocked = tile.blocks_motion();
                for e in &map.entities[idx] {
                    new_motion_blocked = new_motion_blocked
                        || blocks_motion.get_component::<BlocksMotion>(*e).is_ok();
                }
                motion_blocked[idx] = new_motion_blocked;
            }
        }
    }
}

#[derive(Default, Deref, DerefMut)]
struct PreviousMonitorsCollisionsIndex(HashMap<Entity, usize>);

fn monitors_collisions_indexing(
    mut map: Query<(&Map, &mut CollisionsMonitored)>,
    mut prev_index: ResMut<PreviousMonitorsCollisionsIndex>,
    query: Query<
        (Entity, &Coordinates, &MonitorsCollisions),
        Or<(Changed<Coordinates>, Changed<MonitorsCollisions>)>,
    >,
    collision_monitors: Query<&MonitorsCollisions>,
) {
    for (entity, coordinates, _) in query.iter() {
        for (map, mut collisions_monitored) in map.iter_mut() {
            let idx = coordinates.to_index(map.width());
            if let Some(prev_idx) = prev_index.get(&entity) {
                if *prev_idx == idx {
                    continue;
                }
                let mut new_collisions_monitored = false;
                for e in &map.entities[*prev_idx] {
                    if collision_monitors.get(*e).is_ok() {
                        new_collisions_monitored = true;
                        break;
                    }
                }
                collisions_monitored[*prev_idx] = new_collisions_monitored;
            }
            collisions_monitored[idx] = true;
            prev_index.insert(entity, idx);
        }
    }
}

fn remove_monitors_collisions(
    mut prev_index: ResMut<PreviousMonitorsCollisionsIndex>,
    mut map: Query<(&Map, &mut CollisionsMonitored)>,
    removed: RemovedComponents<MonitorsCollisions>,
    coordinates: Query<&Coordinates>,
    monitors_collisions: Query<&MonitorsCollisions>,
) {
    for entity in removed.iter() {
        if let Ok(coordinates) = coordinates.get_component::<Coordinates>(entity) {
            prev_index.remove(&entity);
            for (map, mut collisions_monitored) in map.iter_mut() {
                let idx = (**coordinates).to_index(map.width());
                let mut new_collisions_monitored = false;
                for e in &map.entities[idx] {
                    new_collisions_monitored = new_collisions_monitored
                        || monitors_collisions
                            .get_component::<MonitorsCollisions>(*e)
                            .is_ok();
                }
                collisions_monitored[idx] = new_collisions_monitored;
            }
        }
    }
}

fn add_collision_indices(
    mut commands: Commands,
    query: Query<
        (Entity, &Map),
        (
            Added<Map>,
            Without<MotionBlocked>,
            Without<CollisionsMonitored>,
        ),
    >,
) {
    for (entity, map) in query.iter() {
        let mut v = vec![];
        for tile in &map.base.tiles {
            v.push(tile.blocks_motion());
        }
        commands.entity(entity).insert(MotionBlocked(v));
        let count = (map.width() * map.height()) as usize;
        commands
            .entity(entity)
            .insert(CollisionsMonitored(vec![false; count]));
    }
}

fn speak_direction(
    mut tts: ResMut<Tts>,
    mut cache: Local<HashMap<Entity, CardinalDirection>>,
    player: Query<(Entity, &Player, &Transform), Changed<Transform>>,
) -> Result<(), Box<dyn Error>> {
    if let Ok((entity, _, transform)) = player.single() {
        let forward = transform.local_x();
        let yaw = Angle::Radians(forward.y.atan2(forward.x));
        if let Some(old_direction) = cache.get(&entity) {
            let old_direction = *old_direction;
            let direction: CardinalDirection = yaw.into();
            if old_direction != direction {
                let direction: String = direction.into();
                tts.speak(direction, false)?;
            }
            cache.insert(entity, direction);
        } else {
            cache.insert(entity, yaw.into());
        }
    }
    Ok(())
}

pub const MOVEMENT_LABEL: &str = "MOVEMENT";

#[derive(Clone, Debug)]
pub struct NavigationConfig<S> {
    pub movement_states: Vec<S>,
    pub movement_control_states: Vec<S>,
}

impl<S> Default for NavigationConfig<S> {
    fn default() -> Self {
        Self {
            movement_states: vec![],
            movement_control_states: vec![],
        }
    }
}

pub struct NavigationPlugin<'a, S>(std::marker::PhantomData<&'a S>);

impl<'a, S> Default for NavigationPlugin<'a, S> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<'a, S> Plugin for NavigationPlugin<'a, S>
where
    S: bevy::ecs::component::Component + Clone + Debug + Eq + Hash,
    'a: 'static,
{
    fn build(&self, app: &mut AppBuilder) {
        if !app.world().contains_resource::<NavigationConfig<S>>() {
            app.insert_resource(NavigationConfig::<S>::default());
        }
        let config = app
            .world()
            .get_resource::<NavigationConfig<S>>()
            .unwrap()
            .clone();
        app.register_type::<MaxSpeed>()
            .register_type::<RotationSpeed>()
            .register_type::<Sprinting>()
            .add_event::<Collision>()
            .insert_resource(PreviousBlocksMotionIndex::default())
            .add_system_to_stage(
                CoreStage::PostUpdate,
                blocks_motion_indexing
                    .system()
                    .after(crate::map::UPDATE_ENTITY_INDEX_LABEL)
                    .label(UPDATE_COLLISION_INDEX_LABEL),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                remove_blocks_motion
                    .system()
                    .before(UPDATE_COLLISION_INDEX_LABEL),
            )
            .insert_resource(PreviousMonitorsCollisionsIndex::default())
            .add_system_to_stage(
                CoreStage::PostUpdate,
                monitors_collisions_indexing
                    .system()
                    .after(crate::map::UPDATE_ENTITY_INDEX_LABEL)
                    .label(UPDATE_COLLISION_INDEX_LABEL),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                remove_monitors_collisions
                    .system()
                    .before(UPDATE_COLLISION_INDEX_LABEL),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                monitors_collisions_indexing
                    .system()
                    .after(crate::map::UPDATE_ENTITY_INDEX_LABEL)
                    .label(UPDATE_COLLISION_INDEX_LABEL),
            )
            .add_system(add_collision_indices.system())
            .add_system(speak_direction.system().chain(error_handler.system()))
            .add_system_to_stage(CoreStage::PostUpdate, add_collision_indices.system());
        if config.movement_states.is_empty() {
            app.add_system(
                movement
                    .system()
                    .label(MOVEMENT_LABEL)
                    .before(crate::map::UPDATE_ENTITY_INDEX_LABEL),
            );
        } else {
            let states = config.movement_states;
            for state in states {
                app.add_system_set(
                    SystemSet::on_update(state).with_system(
                        movement
                            .system()
                            .label(MOVEMENT_LABEL)
                            .before(crate::map::UPDATE_ENTITY_INDEX_LABEL),
                    ),
                );
            }
        }
        if config.movement_control_states.is_empty() {
            app.add_system(movement_controls.system().before(MOVEMENT_LABEL));
        } else {
            let states = config.movement_control_states;
            for state in states {
                app.add_system_set(
                    SystemSet::on_update(state)
                        .with_system(movement_controls.system().before(MOVEMENT_LABEL)),
                );
            }
        }
    }
}
