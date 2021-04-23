use std::collections::HashMap;

use bevy::prelude::*;
use derive_more::{Deref, DerefMut};

use crate::{
    core::{Area, Coordinates, PointLike},
    map::Map,
    navigation::{MaxSpeed, Speed},
    pathfinding::Destination,
    visibility::Viewshed,
};

#[derive(Clone, Debug, Deref, DerefMut, Reflect)]
pub struct Pursue(pub Entity);

#[derive(Clone, Debug, Deref, DerefMut, Reflect)]
pub struct PursueInArea(pub Entity);

#[derive(Clone, Debug, Deref, DerefMut, Reflect)]
pub struct PursueWhenVisible(pub Entity);

fn pursue(
    mut commands: Commands,
    map: Query<&Map>,
    mut cache: Local<HashMap<Entity, usize>>,
    pursuers: Query<(Entity, &Pursue)>,
    coordinates: Query<&Coordinates>,
) {
    for (pursuer, pursued) in pursuers.iter() {
        if let Ok(destination) = coordinates.get(pursued.0) {
            for map in map.iter() {
                let destination = Destination(destination.i32());
                let destination_idx = destination.to_index(map.width());
                let cached_destination = cache.get(&pursuer);
                if cached_destination.is_none() || cached_destination != Some(&destination_idx) {
                    cache.insert(pursuer, destination_idx);
                    commands.entity(pursuer).insert(destination);
                }
            }
        }
    }
}

fn pursue_in_area(
    mut commands: Commands,
    pursuer: Query<(Entity, &PursueInArea, &Coordinates)>,
    coordinates: Query<&Coordinates>,
    areas: Query<&Area>,
) {
    for (pursuer_entity, pursuing, pursuer_coordinates) in pursuer.iter() {
        let pursuer_area = areas.iter().find(|v| v.contains(pursuer_coordinates));
        if let Some(pursuer_area) = pursuer_area {
            if let Ok(pursued_coordinates) = coordinates.get(**pursuing) {
                if pursuer_area.contains(pursued_coordinates) {
                    commands.entity(pursuer_entity).insert(Pursue(**pursuing));
                } else {
                    commands.entity(pursuer_entity).remove::<Pursue>();
                }
            }
        }
    }
}

fn pursue_when_visible(
    mut commands: Commands,
    mut pursuer: Query<(Entity, &PursueWhenVisible, &Viewshed, &mut Speed, &MaxSpeed)>,
    coordinates: Query<&Coordinates>,
    mut cache: Local<HashMap<Entity, (i32, i32)>>,
) {
    for (pursuer_entity, pursuing, viewshed, mut speed, max_speed) in pursuer.iter_mut() {
        if let Ok(pursued_coordinates) = coordinates.get(**pursuing) {
            let mut update_destination = false;
            if !cache.contains_key(&**pursuing) {
                cache.insert(**pursuing, pursued_coordinates.i32());
                update_destination = true;
            } else {
                if let Some(prev_coordinates) = cache.get(&**pursuing) {
                    if prev_coordinates.i32() != pursued_coordinates.i32() {
                        update_destination = true;
                    }
                }
            }
            if viewshed.is_visible(pursued_coordinates) && update_destination {
                commands
                    .entity(pursuer_entity)
                    .insert(Destination(pursued_coordinates.i32()));
                **speed = **max_speed;
            }
        }
    }
}

pub struct BehaviorPlugin;

impl Plugin for BehaviorPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.register_type::<Pursue>()
            .register_type::<PursueInArea>()
            .register_type::<PursueWhenVisible>()
            .add_system_to_stage(CoreStage::PreUpdate, pursue.system())
            .add_system_to_stage(CoreStage::PreUpdate, pursue_in_area.system())
            .add_system_to_stage(CoreStage::PreUpdate, pursue_when_visible.system());
    }
}
