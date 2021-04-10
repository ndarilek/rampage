use std::{collections::HashMap, time::Duration};

use bevy::{
    asset::{HandleId, LoadState},
    prelude::*,
    transform::TransformSystem,
};
use bevy_openal::{Buffer, Sound, SoundState};

use rand::random;

use crate::{core::Player, exploration::ExplorationFocused, visibility::Viewshed};

#[derive(Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct Footstep {
    pub sound: HandleId,
    pub step_length: f32,
    pub gain: f32,
}

impl Default for Footstep {
    fn default() -> Self {
        Self {
            sound: "".into(),
            step_length: 0.8,
            gain: 0.05,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SoundIcon {
    pub sound: HandleId,
    pub gain: f32,
    pub pitch: f32,
    pub interval: Option<Timer>,
}

impl Default for SoundIcon {
    fn default() -> Self {
        let seconds = random::<f32>() + 4.5;
        let mut icon = Self {
            sound: "".into(),
            gain: 0.3,
            pitch: 1.,
            interval: Some(Timer::from_seconds(seconds, true)),
        };
        if let Some(ref mut interval) = icon.interval {
            let seconds = Duration::from_secs_f32(seconds - 0.1);
            interval.set_elapsed(seconds);
        }
        icon
    }
}

#[derive(Bundle, Default)]
pub struct FootstepBundle {
    pub footstep: Footstep,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
}

#[derive(Bundle, Clone, Debug, Default)]
pub struct SoundIconBundle {
    pub sound_icon: SoundIcon,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
}

fn footstep(
    mut commands: Commands,
    assets: Res<Assets<Buffer>>,
    mut last_step_distance: Local<HashMap<Entity, (f32, Vec3)>>,
    footsteps: Query<
        (Entity, &Footstep, Option<&Children>, &GlobalTransform),
        Changed<GlobalTransform>,
    >,
    mut sounds: Query<&mut Sound>,
) {
    for (entity, footstep, children, transform) in footsteps.iter() {
        if let Some(children) = children {
            if let Some(last) = last_step_distance.get(&entity) {
                let distance = last.0 + (transform.translation - last.1).length();
                if distance >= footstep.step_length {
                    last_step_distance.insert(entity, (0., transform.translation));
                    let sound = children[0];
                    if let Ok(mut sound) = sounds.get_mut(sound) {
                        sound.gain = footstep.gain;
                        sound.play();
                    }
                } else if last.1 != transform.translation {
                    last_step_distance.insert(entity, (distance, transform.translation));
                }
            } else {
                last_step_distance.insert(entity, (0., transform.translation));
            }
        } else {
            let buffer = assets.get_handle(footstep.sound);
            let sound = Sound {
                buffer,
                state: SoundState::Stopped,
                gain: footstep.gain,
                ..Default::default()
            };
            let child = commands
                .spawn()
                .insert(sound)
                .insert(Transform::default())
                .insert(GlobalTransform::default())
                .id();
            commands.entity(entity).push_children(&[child]);
        }
    }
}

fn sound_icon(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    viewers: Query<(&Player, &Viewshed)>,
    mut icons: Query<(
        Entity,
        &mut SoundIcon,
        &Transform,
        Option<&GlobalTransform>,
        Option<&Children>,
    )>,
    mut sounds: Query<&mut Sound>,
) {
    for (_, viewer) in viewers.iter() {
        for (entity, mut icon, transform, global_transform, children) in icons.iter_mut() {
            let translation = global_transform
                .map(|v| v.translation)
                .unwrap_or_else(|| transform.translation);
            let (x, y) = (translation.x, translation.y);
            let x = x as i32;
            let y = y as i32;
            if viewer.visible.contains(&(x, y)) {
                let buffer = asset_server.get_handle(icon.sound);
                if asset_server.get_load_state(&buffer) == LoadState::Loaded {
                    let looping = icon.interval.is_none();
                    let sound = Sound {
                        buffer,
                        gain: icon.gain,
                        looping,
                        state: SoundState::Playing,
                        ..Default::default()
                    };
                    if looping && children.is_none() {
                        let child = commands
                            .spawn()
                            .insert(sound)
                            .insert(Transform::default())
                            .insert(GlobalTransform::default())
                            .id();
                        commands.entity(entity).push_children(&[child]);
                    } else {
                        if let Some(ref mut interval) = icon.interval {
                            interval.tick(time.delta());
                            if interval.finished() {
                                if let Some(children) = children {
                                    for child in children.iter() {
                                        commands.entity(*child).despawn();
                                    }
                                }
                                let child = commands
                                    .spawn()
                                    .insert(sound)
                                    .insert(Transform::default())
                                    .insert(GlobalTransform::default())
                                    .id();
                                commands.entity(entity).push_children(&[child]);
                                interval.reset();
                            } else if let Some(children) = children {
                                if let Some(child) = children.get(0) {
                                    if let Ok(mut sound) = sounds.get_mut(*child) {
                                        sound.gain = icon.gain;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn sound_icon_exploration_focus_changed(
    mut focused: Query<(&ExplorationFocused, Option<&mut SoundIcon>), Changed<ExplorationFocused>>,
) {
    for (_, icon) in focused.iter_mut() {
        if let Some(mut icon) = icon {
            icon.gain *= 3.;
        }
    }
}

fn sound_icon_exploration_focus_removed(
    removed: RemovedComponents<ExplorationFocused>,
    mut icons: Query<&mut SoundIcon>,
) {
    for entity in removed.iter() {
        if let Ok(mut icon) = icons.get_component_mut::<SoundIcon>(entity) {
            icon.gain /= 3.;
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut AppBuilder) {
        const SOUND_ICON_AND_EXPLORATION_STAGE: &str = "sound_icon_and_exploration";
        app.register_type::<Footstep>()
            .add_system_to_stage(
                CoreStage::PostUpdate,
                footstep.system().after(TransformSystem::TransformPropagate),
            )
            .add_system(sound_icon.system())
            .add_stage_after(
                CoreStage::PostUpdate,
                SOUND_ICON_AND_EXPLORATION_STAGE,
                SystemStage::parallel(),
            )
            .add_system_to_stage(
                SOUND_ICON_AND_EXPLORATION_STAGE,
                sound_icon_exploration_focus_changed.system(),
            )
            .add_system_to_stage(
                SOUND_ICON_AND_EXPLORATION_STAGE,
                sound_icon_exploration_focus_removed.system(),
            );
    }
}
