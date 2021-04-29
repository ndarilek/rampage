use std::{collections::HashMap, time::Duration};

use bevy::{
    asset::{HandleId, LoadState},
    prelude::*,
    transform::TransformSystem,
};
use bevy_openal::{Buffer, Context, Sound, SoundState};

use rand::random;

use crate::{
    core::{Coordinates, CoreConfig, Player, PointLike},
    exploration::ExplorationFocused,
    visibility::Viewshed,
};

#[derive(Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct Footstep {
    pub sound: HandleId,
    pub step_length: f32,
    pub gain: f32,
    pub reference_distance: f32,
    pub max_distance: f32,
    pub rolloff_factor: f32,
    pub pitch_variation: Option<f32>,
}

impl Default for Footstep {
    fn default() -> Self {
        Self {
            sound: "".into(),
            step_length: 0.8,
            gain: 0.05,
            reference_distance: 1.,
            max_distance: f32::MAX,
            rolloff_factor: 1.,
            pitch_variation: Some(0.15),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SoundIcon {
    pub sound: HandleId,
    pub gain: f32,
    pub pitch: f32,
    pub reference_distance: f32,
    pub max_distance: f32,
    pub rolloff_factor: f32,
    pub interval: Option<Timer>,
}

impl Default for SoundIcon {
    fn default() -> Self {
        let seconds = random::<f32>() + 4.5;
        let mut icon = Self {
            sound: "".into(),
            gain: 0.3,
            pitch: 1.,
            reference_distance: 1.,
            max_distance: f32::MAX,
            rolloff_factor: 1.,
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
    mut last_step_distance: Local<HashMap<Entity, (f32, Coordinates)>>,
    footsteps: Query<(Entity, &Footstep, &Parent, Option<&Children>), Changed<GlobalTransform>>,
    coordinates_storage: Query<&Coordinates>,
    mut sounds: Query<&mut Sound>,
) {
    for (entity, footstep, parent, children) in footsteps.iter() {
        let coordinates = coordinates_storage.get(**parent).unwrap();
        if let Some(children) = children {
            if let Some(last) = last_step_distance.get(&entity) {
                let distance = last.0 + (last.1.distance(coordinates));
                if distance >= footstep.step_length {
                    last_step_distance.insert(entity, (0., *coordinates));
                    let sound = children[0];
                    if let Ok(mut sound) = sounds.get_mut(sound) {
                        sound.gain = footstep.gain;
                        sound.reference_distance = footstep.reference_distance;
                        sound.max_distance = footstep.max_distance;
                        sound.rolloff_factor = footstep.rolloff_factor;
                        if let Some(pitch_variation) = footstep.pitch_variation {
                            let mut pitch = 1. - pitch_variation / 2.;
                            pitch += random::<f32>() * pitch_variation;
                            sound.pitch = pitch;
                        }
                        sound.play();
                    }
                } else if last.1 != *coordinates {
                    last_step_distance.insert(entity, (distance, *coordinates));
                }
            } else {
                last_step_distance.insert(entity, (0., *coordinates));
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
        Option<&Coordinates>,
        Option<&Parent>,
        Option<&Children>,
    )>,
    coordinates_storage: Query<&Coordinates>,
    mut sounds: Query<&mut Sound>,
) {
    for (_, viewer) in viewers.iter() {
        for (entity, mut icon, coordinates, parent, children) in icons.iter_mut() {
            let coords = if let Some(coordinates) = coordinates {
                *coordinates
            } else if let Some(parent) = parent {
                *coordinates_storage
                    .get(**parent)
                    .expect("If `SoundIcon` is a child, its parent must have `Coordinates`")
            } else {
                panic!("No `Coordinates` on `SoundIcon` or parent");
            };
            if viewer.is_visible(&coords) {
                let buffer = asset_server.get_handle(icon.sound);
                if asset_server.get_load_state(&buffer) == LoadState::Loaded {
                    let looping = icon.interval.is_none();
                    let sound = Sound {
                        buffer,
                        gain: icon.gain,
                        pitch: icon.pitch,
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
                    } else if let Some(ref mut interval) = icon.interval {
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
                        }
                    }
                    if let Some(children) = children {
                        if let Some(child) = children.get(0) {
                            if let Ok(mut sound) = sounds.get_mut(*child) {
                                let buffer = asset_server.get_handle(icon.sound);
                                if sound.buffer != buffer {
                                    sound.stop();
                                    sound.buffer = buffer;
                                    sound.play();
                                }
                                sound.gain = icon.gain;
                                sound.pitch = icon.pitch;
                                sound.reference_distance = icon.reference_distance;
                                sound.max_distance = icon.max_distance;
                                sound.rolloff_factor = icon.rolloff_factor;
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

fn scale_sounds(config: Res<CoreConfig>, mut sounds: Query<&mut Sound>) {
    let pixels_per_unit = config.pixels_per_unit as f32;
    for mut sound in sounds.iter_mut() {
        sound.reference_distance *= pixels_per_unit;
        if sound.max_distance != f32::MAX {
            sound.max_distance *= pixels_per_unit;
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut AppBuilder) {
        const SOUND_ICON_AND_EXPLORATION_STAGE: &str = "sound_icon_and_exploration";
        let config = *app.world().get_resource::<CoreConfig>().unwrap();
        if let Some(context) = app.world().get_resource::<Context>() {
            context
                .set_meters_per_unit(1. / config.pixels_per_unit as f32)
                .unwrap();
        }
        app.register_type::<Footstep>()
            .add_system_to_stage(
                CoreStage::PostUpdate,
                footstep.system().after(TransformSystem::TransformPropagate),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                sound_icon
                    .system()
                    .after(TransformSystem::TransformPropagate),
            )
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
            )
            .add_system(scale_sounds.system());
    }
}
