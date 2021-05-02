use std::collections::HashMap;

use bevy::prelude::*;
use blackout::{
    bevy_openal::{Buffer, Sound, SoundState},
    core::{Coordinates, Player, PointLike},
    derive_more::{Deref, DerefMut},
    log::Log,
    map::Map,
    mapgen::TileType,
    navigation::Velocity,
    rand::prelude::*,
};

use crate::{
    bonus::AwardBonus,
    game::{AppState, Sfx},
    player::LifeLost,
    robot::{CauseOfDeath, Robot, RobotKilled},
};

#[derive(Clone, Copy, Debug)]
pub struct Bullet(pub Entity);

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct ShotTimer(pub Timer);

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
pub struct ShotRange(pub u32);

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct ShotSpeed(pub u32);

#[derive(Bundle, Default)]
pub struct BulletBundle {
    pub coordinates: Coordinates,
    pub range: ShotRange,
    pub velocity: Velocity,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub sound: Sound,
}

fn bullet(
    mut commands: Commands,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    mut bullets: Query<(&Bullet, Entity, &Coordinates, &ShotRange, &mut Sound)>,
    mut active_bullets: Local<HashMap<Entity, ((f32, f32), f32)>>,
    robots: Query<(&Robot, Entity, &Coordinates)>,
    level: Query<(Entity, &Map)>,
    mut robot_killed: EventWriter<RobotKilled>,
    mut bonus: EventWriter<AwardBonus>,
    player: Query<(&Player, Entity, &Coordinates)>,
    mut log: Query<&mut Log>,
    mut life_lost: EventWriter<LifeLost>,
) {
    for (bullet, entity, coordinates, range, mut sound) in bullets.iter_mut() {
        if !active_bullets.contains_key(&entity) {
            active_bullets.insert(entity, ((coordinates.x(), coordinates.y()), 0.));
        }
        if sound.state != SoundState::Playing {
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
        for (Robot(robot_type), entity, robot_coordinates) in robots.iter() {
            if *owner != entity && coordinates.distance(robot_coordinates) <= 0.75 {
                if let Ok((_, map)) = level.single() {
                    let index = robot_coordinates.to_index(map.width());
                    robot_killed.send(RobotKilled(
                        entity,
                        *robot_type,
                        *robot_coordinates,
                        index,
                        CauseOfDeath::Bullet(*owner),
                    ));
                    bonus.send(AwardBonus);
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

pub struct BulletPlugin;

impl Plugin for BulletPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_system_set(SystemSet::on_update(AppState::InGame).with_system(bullet.system()));
    }
}
