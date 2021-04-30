use std::{error::Error, f32::consts::PI};

use bevy::prelude::*;
use blackout::{
    bevy_input_actionmap::InputMap,
    bevy_openal::{Buffer, Listener, Sound, SoundState},
    bevy_tts::Tts,
    core::{Angle, Area, Coordinates, MovementDirection, Player, PointLike},
    derive_more::{Deref, DerefMut},
    error::error_handler,
    exploration::Mappable,
    log::Log,
    map::{Areas, Map},
    navigation::{BlocksMotion, MaxSpeed, RotationSpeed, Speed, Velocity},
    sound::{Footstep, FootstepBundle},
    visibility::{BlocksVisibility, Viewshed},
};

use crate::{
    bonus::BonusTimes,
    bullet::{Bullet, BulletBundle, ShotRange, ShotSpeed, ShotTimer},
    game::{
        AppState, Reset, Sfx, SHOOT, SNAP_LEFT, SNAP_RIGHT, SPEAK_COORDINATES, SPEAK_DIRECTION,
        SPEAK_HEALTH, SPEAK_LEVEL, SPEAK_ROBOT_COUNT, SPEAK_SCORE,
    },
    level::Level,
    robot::{Robot, RobotKilled, RobotType},
};

#[derive(Clone, Debug, Deref, DerefMut)]
struct BetweenLivesTimer(Timer);

impl Default for BetweenLivesTimer {
    fn default() -> Self {
        BetweenLivesTimer(Timer::from_seconds(5., false))
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Checkpoint(Coordinates, Quat);

pub struct LifeLost;

#[derive(Clone, Copy, Debug, Deref, DerefMut)]
pub struct Lives(pub u32);

impl Default for Lives {
    fn default() -> Self {
        Lives(3)
    }
}

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
pub struct Score(pub u32);

#[derive(Clone, Copy, Debug, Default)]
pub struct Shoot;

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
    score: Score,
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
            shot_timer: ShotTimer(Timer::from_seconds(0.1, false)),
            shot_range: ShotRange(24),
            shot_speed: ShotSpeed(36),
            level: Default::default(),
            score: Default::default(),
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

fn speak_info(
    input: Res<InputMap<String>>,
    mut tts: ResMut<Tts>,
    player: Query<(&Player, &Coordinates, &Transform, &Lives, &Level, &Score)>,
    robots: Query<&Robot>,
) -> Result<(), Box<dyn Error>> {
    if input.just_active(SPEAK_COORDINATES) {
        if let Ok((_, coordinates, _, _, _, _)) = player.single() {
            tts.speak(
                format!("({}, {})", coordinates.x_i32(), coordinates.y_i32()),
                true,
            )?;
        }
    }
    if input.just_active(SPEAK_DIRECTION) {
        if let Ok((_, _, transform, _, _, _)) = player.single() {
            let forward = transform.local_x();
            let yaw = Angle::Radians(forward.y.atan2(forward.x));
            let direction: MovementDirection = yaw.into();
            tts.speak(format!("{}", direction), true)?;
        }
    }
    if input.just_active(SPEAK_HEALTH) {
        if let Ok((_, _, _, lives, _, _)) = player.single() {
            let life_or_lives = if **lives != 1 { "lives" } else { "life" };
            tts.speak(format!("{} {} left.", **lives, life_or_lives), true)?;
        }
    }
    if input.just_active(SPEAK_LEVEL) {
        if let Ok((_, _, _, _, level, _)) = player.single() {
            tts.speak(format!("Level {}", **level), true)?;
        }
    }
    if input.just_active(SPEAK_SCORE) {
        if let Ok((_, _, _, _, _, score)) = player.single() {
            let point_or_points = if **score == 1 { "point" } else { "points" };
            tts.speak(format!("{} {}.", **score, point_or_points), true)?;
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
    mut shoot: EventWriter<Shoot>,
    level: Query<(Entity, &Map)>,
    sfx: Res<Sfx>,
    buffers: Res<Assets<Buffer>>,
) {
    if let Ok((_, player_entity, coordinates, transform, mut timer, shot_range, shot_speed)) =
        player.single_mut()
    {
        timer.tick(time.delta());
        if input.active(SHOOT) && timer.finished() {
            shoot.send(Shoot);
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

fn score(
    mut score: Query<&mut Score>,
    mut shot: EventReader<Shoot>,
    mut shots_fired: Local<u8>,
    mut robot_kills: EventReader<RobotKilled>,
    active_bonuses: Query<&BonusTimes>,
) {
    const SHOTS_PER_POINT: u8 = 5;
    if let Ok(mut score) = score.single_mut() {
        for _ in shot.iter() {
            *shots_fired += 1;
            if **score > 0 && *shots_fired > SHOTS_PER_POINT {
                **score -= 1;
                *shots_fired = 0;
            }
        }
        for RobotKilled(_, robot_type, _, _, _) in robot_kills.iter() {
            let mut points: f32 = match robot_type {
                RobotType::Dumbass => 10.,
                RobotType::Jackass => 50.,
                RobotType::Badass => 100.,
            };
            if let Ok(active_bonuses) = active_bonuses.single() {
                if !active_bonuses.is_empty() {
                    for _ in &active_bonuses[1..] {
                        points *= 1.2;
                    }
                }
            }
            **score += points as u32;
        }
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.init_resource::<BetweenLivesTimer>()
            .add_event::<LifeLost>()
            .add_event::<Shoot>()
            .add_system_set(
                SystemSet::on_exit(AppState::Loading).with_system(spawn_player.system()),
            )
            .add_system_set(
                SystemSet::on_exit(AppState::GameOver).with_system(spawn_player.system()),
            )
            .add_system_set(
                SystemSet::on_update(AppState::InGame)
                    .with_system(speak_info.system().chain(error_handler.system()))
                    .with_system(snap.system())
                    .with_system(shoot.system()),
            )
            .add_system(checkpoint.system())
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
            .add_system(score.system());
    }
}
