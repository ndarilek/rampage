use std::{
    collections::{HashMap, HashSet},
    f32::consts::PI,
};

use bevy::{ecs::system::EntityCommands, prelude::*};
use big_brain::prelude::*;
use blackout::{
    bevy_openal::{Buffer, Sound, SoundState},
    core::{Coordinates, Player, PointLike},
    derive_more::{Deref, DerefMut},
    log::Log,
    map::{Areas, Map},
    navigation::{BlocksMotion, MaxSpeed, MotionBlocked, Speed, Velocity},
    pathfinding::Destination,
    rand::prelude::*,
    sound::{Footstep, FootstepBundle, SoundIcon, SoundIconBundle},
    visibility::{BlocksVisibility, Viewshed, VisibilityBlocked},
};

use crate::{
    bonus::AwardBonus,
    bullet::{Bullet, BulletBundle, ShotRange, ShotSpeed, ShotTimer},
    game::{AppState, Sfx, Sprites},
    level::WallCollision,
};

pub enum CauseOfDeath {
    Bullet(Entity),
    Shockwave(Name),
}

#[derive(Clone, Copy, Debug)]
pub struct Curious;

impl Curious {
    pub fn build() -> CuriousBuilder {
        CuriousBuilder
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CuriousBuilder;

impl ScorerBuilder for CuriousBuilder {
    fn build(&self, cmd: &mut Commands, scorer: Entity, _actor: Entity) {
        cmd.entity(scorer).insert(Curious);
    }
}

#[derive(Clone, Debug)]
pub struct DeathTimer(pub Timer, pub Name);

#[derive(Clone, Copy, Debug)]
pub struct Investigate;

impl Investigate {
    pub fn build() -> InvestigateBuilder {
        InvestigateBuilder
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InvestigateBuilder;

impl ActionBuilder for InvestigateBuilder {
    fn build(&self, cmd: &mut Commands, action: Entity, _actor: Entity) {
        cmd.entity(action).insert(Investigate);
    }
}

#[derive(Clone, Copy, Debug, Deref, DerefMut)]
struct InvestigateCoordinates((i32, i32));

#[derive(Clone, Copy, Debug)]
pub struct PursuePlayer;

impl PursuePlayer {
    pub fn build() -> PursuePlayerBuilder {
        PursuePlayerBuilder
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PursuePlayerBuilder;

impl ActionBuilder for PursuePlayerBuilder {
    fn build(&self, cmd: &mut Commands, action: Entity, _actor: Entity) {
        cmd.entity(action).insert(PursuePlayer);
    }
}

pub struct RobotKilled(
    pub Entity,
    pub RobotType,
    pub Coordinates,
    pub usize,
    pub CauseOfDeath,
);

#[derive(Clone, Copy, Debug)]
pub enum RobotType {
    Dumbass,
    Jackass,
    Badass,
}

#[derive(Clone, Copy, Debug)]
pub struct Robot(pub RobotType);

#[derive(Clone, Copy, Debug)]
pub struct SeesPlayer;

impl SeesPlayer {
    pub fn build() -> SeesPlayerBuilder {
        SeesPlayerBuilder
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SeesPlayerBuilder;

impl ScorerBuilder for SeesPlayerBuilder {
    fn build(&self, cmd: &mut Commands, scorer: Entity, _actor: Entity) {
        cmd.entity(scorer).insert(SeesPlayer);
    }
}

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
pub struct ShotAccuracy(pub f32);

#[derive(Bundle)]
pub struct RobotBundle {
    pub robot: Robot,
    pub coordinates: Coordinates,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub speed: Speed,
    pub max_speed: MaxSpeed,
    pub velocity: Velocity,
    pub name: Name,
    pub viewshed: Viewshed,
    pub blocks_visibility: BlocksVisibility,
    pub blocks_motion: BlocksMotion,
    pub shot_timer: ShotTimer,
    pub shot_range: ShotRange,
    pub shot_speed: ShotSpeed,
    pub shot_accuracy: ShotAccuracy,
}

pub trait RobotCommands<'a, 'b> {
    fn insert_robot(&mut self, robot_type: &RobotType) -> &mut EntityCommands<'a, 'b>;
}

impl<'a, 'b> RobotCommands<'a, 'b> for EntityCommands<'a, 'b> {
    fn insert_robot(&mut self, robot_type: &RobotType) -> &mut Self {
        let max_speed;
        let visibility_range;
        let shot_accuracy;
        match robot_type {
            RobotType::Dumbass => {
                max_speed = MaxSpeed(2.);
                visibility_range = 12;
                shot_accuracy = ShotAccuracy(PI / 9.);
            }
            RobotType::Jackass => {
                max_speed = MaxSpeed(4.);
                visibility_range = 16;
                shot_accuracy = ShotAccuracy(PI / 10.);
            }
            RobotType::Badass => {
                max_speed = MaxSpeed(4.);
                visibility_range = 24;
                shot_accuracy = ShotAccuracy(PI / 12.);
            }
        };
        self.insert_bundle(RobotBundle {
            robot: Robot(*robot_type),
            transform: Default::default(),
            global_transform: Default::default(),
            speed: Default::default(),
            max_speed,
            velocity: Default::default(),
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
            coordinates: Default::default(),
            name: Default::default(),
        })
        .insert(
            Thinker::build()
                .picker(FirstToScore { threshold: 0.8 })
                .when(SeesPlayer::build(), PursuePlayer::build())
                .when(Curious::build(), Investigate::build()),
        )
        .with_children(|parent| {
            parent
                .spawn()
                .insert(Transform::default())
                .insert(GlobalTransform::default())
                .insert(Timer::from_seconds(10., false));
        })
    }
}

fn post_process_robots(
    mut commands: Commands,
    sfx: Res<Sfx>,
    sprites: Res<Sprites>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    robots: Query<(&Robot, Entity), Added<Robot>>,
) {
    for (Robot(robot_type), entity) in robots.iter() {
        let sprite_handle = asset_server.get_handle(match robot_type {
            RobotType::Dumbass => sprites.dumbass,
            RobotType::Jackass => sprites.jackass,
            RobotType::Badass => sprites.badass,
        });
        commands.entity(entity).insert_bundle(SpriteBundle {
            material: materials.add(sprite_handle.into()),
            ..Default::default()
        });
        let footstep = commands
            .spawn()
            .insert_bundle(FootstepBundle {
                footstep: Footstep {
                    sound: sfx.robot_footstep,
                    step_length: 2.,
                    gain: 1.2,
                    reference_distance: 5.,
                    rolloff_factor: 1.5,
                    pitch_variation: None,
                    ..Default::default()
                },
                ..Default::default()
            })
            .id();
        let sound_icon = commands
            .spawn()
            .insert_bundle(SoundIconBundle {
                sound_icon: SoundIcon {
                    sound: match robot_type {
                        RobotType::Dumbass => sfx.robot_dumbass,
                        RobotType::Jackass => sfx.robot_jackass,
                        RobotType::Badass => sfx.robot_badass,
                    },
                    gain: 0.8,
                    ..Default::default()
                },
                ..Default::default()
            })
            .id();
        commands
            .entity(entity)
            .push_children(&[footstep, sound_icon]);
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

fn pursue_player(
    mut commands: Commands,
    mut query: Query<(&Actor, &mut ActionState), With<PursuePlayer>>,
    player: Query<(&Player, &Coordinates)>,
    mut log: Query<&mut Log>,
    names: Query<&Name>,
    robot: Query<&MaxSpeed>,
    children: Query<&Children>,
    mut timers: Query<&mut Timer>,
) {
    for (Actor(actor), mut state) in query.iter_mut() {
        match *state {
            ActionState::Requested => {
                if let Ok(children) = children.get(*actor) {
                    let voice_entity = children[0];
                    if let Ok(mut timer) = timers.get_mut(voice_entity) {
                        timer.reset();
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

const VOICE_GAIN: f32 = 1.2;
const VOICE_REFERENCE_DISTANCE: f32 = 4.;

fn comment_on_investigation(
    mut commands: Commands,
    query: Query<&Actor, With<Investigate>>,
    time: Res<Time>,
    robots: Query<(&Robot, &Children)>,
    mut timers: Query<&mut Timer>,
    mut sounds: Query<&mut Sound>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
) {
    for Actor(actor) in query.iter() {
        if let Ok((_, children)) = robots.get(*actor) {
            let voice = children[0];
            if let Ok(mut timer) = timers.get_mut(voice) {
                if timer.percent() == 0. {
                    if let Ok(mut sound) = sounds.get_mut(voice) {
                        sound.stop();
                    }
                    let mut comments = sfx.investigate.clone();
                    comments.shuffle(&mut thread_rng());
                    let buffer = buffers.get_handle(comments[0]);
                    let sound = Sound {
                        buffer,
                        state: SoundState::Playing,
                        gain: VOICE_GAIN,
                        reference_distance: VOICE_REFERENCE_DISTANCE,
                        ..Default::default()
                    };
                    commands.entity(voice).insert(sound);
                }
                timer.tick(time.delta());
                if timer.finished() {
                    timer.reset();
                }
            }
        }
    }
}

fn taunt_player(
    mut commands: Commands,
    query: Query<&Actor, With<PursuePlayer>>,
    time: Res<Time>,
    robots: Query<(&Robot, &Children)>,
    mut timers: Query<&mut Timer>,
    mut sounds: Query<&mut Sound>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
) {
    for Actor(actor) in query.iter() {
        if let Ok((_, children)) = robots.get(*actor) {
            let voice = children[0];
            if let Ok(mut timer) = timers.get_mut(voice) {
                if timer.percent() == 0. {
                    if let Ok(mut sound) = sounds.get_mut(voice) {
                        sound.stop();
                    }
                    let mut comments = sfx.taunts.clone();
                    comments.shuffle(&mut thread_rng());
                    let buffer = buffers.get_handle(comments[0]);
                    let sound = Sound {
                        buffer,
                        state: SoundState::Playing,
                        gain: VOICE_GAIN,
                        reference_distance: VOICE_REFERENCE_DISTANCE,
                        ..Default::default()
                    };
                    commands.entity(voice).insert(sound);
                }
                timer.tick(time.delta());
                if timer.finished() {
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

fn investigate_coordinates(
    mut commands: Commands,
    actors: Query<(Entity, &Viewshed, &Coordinates), With<Robot>>,
    bullets: Query<(&Bullet, Entity, &Coordinates)>,
    mut seen_bullets: Local<HashMap<Entity, HashSet<Entity>>>,
    mut robot_kills: EventReader<RobotKilled>,
    level: Query<(&Map, &MotionBlocked, &Areas)>,
    mut wall_collisions: EventReader<WallCollision>,
) {
    let mut investigations: Vec<(i32, i32)> = vec![];
    let mut rng = thread_rng();
    for (actor_entity, viewshed, _) in actors.iter() {
        if !seen_bullets.contains_key(&actor_entity) {
            seen_bullets.insert(actor_entity, HashSet::new());
        }
        for (_, bullet_entity, bullet_coordinates) in bullets.iter() {
            if let Some(seen_bullets) = seen_bullets.get_mut(&actor_entity) {
                if !seen_bullets.contains(&bullet_entity) && viewshed.is_visible(bullet_coordinates)
                {
                    if let Ok((map, motion_blocked, areas)) = level.single() {
                        if motion_blocked[bullet_coordinates.to_index(map.width())] {
                            if let Some(area) =
                                areas.iter().find(|a| a.contains(bullet_coordinates))
                            {
                                loop {
                                    let coords = (
                                        rng.gen_range(area.rect.x1..area.rect.x2) as i32,
                                        rng.gen_range(area.rect.y1..area.rect.y2) as i32,
                                    );
                                    if !investigations.contains(&coords)
                                        || !motion_blocked[coords.to_index(map.width())]
                                    {
                                        commands
                                            .entity(actor_entity)
                                            .insert(InvestigateCoordinates(coords));
                                        investigations.push(coords);
                                        break;
                                    }
                                }
                            }
                        } else {
                            commands
                                .entity(actor_entity)
                                .insert(InvestigateCoordinates(bullet_coordinates.i32()));
                        }
                    }
                    seen_bullets.insert(bullet_entity);
                }
            }
        }
    }
    for RobotKilled(_, _, old_robot_coords, _, _) in robot_kills.iter() {
        for (entity, _, robot_coords) in actors.iter() {
            if robot_coords.distance(old_robot_coords) <= 20. {
                if let Ok((map, motion_blocked, areas)) = level.single() {
                    if let Some(area) = areas.iter().find(|a| a.contains(old_robot_coords)) {
                        loop {
                            let coords = (
                                rng.gen_range(area.rect.x1..area.rect.x2) as i32,
                                rng.gen_range(area.rect.y1..area.rect.y2) as i32,
                            );
                            if !investigations.contains(&coords)
                                || !motion_blocked[coords.to_index(map.width())]
                            {
                                commands
                                    .entity(entity)
                                    .insert(InvestigateCoordinates(coords));
                                investigations.push(coords);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    for WallCollision(coords) in wall_collisions.iter() {
        for (entity, _, robot_coords) in actors.iter() {
            if robot_coords.distance(coords) <= 30. {
                if let Ok((map, motion_blocked, areas)) = level.single() {
                    if let Some(area) = areas.iter().find(|a| a.contains(coords)) {
                        loop {
                            let coords = (
                                rng.gen_range(area.rect.x1..area.rect.x2) as i32,
                                rng.gen_range(area.rect.y1..area.rect.y2) as i32,
                            );
                            if !investigations.contains(&coords)
                                || !motion_blocked[coords.to_index(map.width())]
                            {
                                commands
                                    .entity(entity)
                                    .insert(InvestigateCoordinates(coords));
                                investigations.push(coords);
                                break;
                            }
                        }
                    }
                }
            }
        }
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

fn investigate(
    mut commands: Commands,
    mut query: Query<(&Actor, &mut ActionState), With<Investigate>>,
    investigations: Query<&InvestigateCoordinates>,
    max_speeds: Query<&MaxSpeed>,
    destinations: Query<&Destination>,
    viewsheds: Query<&Viewshed>,
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
                            if let Ok(viewshed) = viewsheds.get(*actor) {
                                if viewshed.is_visible(coordinates) {
                                    *state = ActionState::Success;
                                }
                            }
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
    for RobotKilled(entity, _, _, index, cause) in events.iter() {
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
                    if distance <= 7.5 {
                        if let Ok(name) = names.get(*entity) {
                            commands.entity(candidate_entity).insert(DeathTimer(
                                Timer::from_seconds(distance / 5., false),
                                name.clone(),
                            ));
                            let sound = commands
                                .spawn()
                                .insert(Sound {
                                    buffer: buffers.get_handle(sfx.shockwave),
                                    state: SoundState::Playing,
                                    looping: true,
                                    reference_distance: 3.,
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
    mut exploding: Query<(Entity, &Robot, &Coordinates, &mut DeathTimer, &Children)>,
    mut sounds: Query<&mut Sound>,
    level: Query<&Map>,
    mut robot_killed: EventWriter<RobotKilled>,
    mut bonus: EventWriter<AwardBonus>,
) {
    for (entity, Robot(robot_type), coordinates, mut timer, children) in exploding.iter_mut() {
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
                    *robot_type,
                    *coordinates,
                    index,
                    CauseOfDeath::Shockwave(timer.1.clone()),
                ));
                bonus.send(AwardBonus);
            }
        }
    }
}

pub struct RobotPlugin;

impl Plugin for RobotPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_event::<RobotKilled>()
            .add_plugin(BigBrainPlugin)
            .add_system(post_process_robots.system())
            .add_system(sees_player_scorer.system())
            .add_system_to_stage(CoreStage::PreUpdate, pursue_player.system())
            .add_system_to_stage(CoreStage::PostUpdate, comment_on_investigation.system())
            .add_system_to_stage(CoreStage::PostUpdate, taunt_player.system())
            .add_system_to_stage(CoreStage::PreUpdate, investigate_coordinates.system())
            .add_system(curious_scorer.system())
            .add_system_to_stage(CoreStage::PreUpdate, investigate.system())
            .add_system_set(
                SystemSet::on_update(AppState::InGame)
                    .with_system(shoot_player.system())
                    .with_system(shockwave.system()),
            )
            .add_system(robot_killed.system());
    }
}
