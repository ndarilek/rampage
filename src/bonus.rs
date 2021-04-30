use std::time::Instant;

use bevy::prelude::*;

use blackout::{
    bevy_openal::{Buffer, Sound, SoundState},
    derive_more::{Deref, DerefMut},
    map::Map,
};

use crate::game::{Reset, Sfx};

pub struct AwardBonus;

#[derive(Clone, Debug, Default, Deref, DerefMut)]
struct BonusTimes(Vec<Instant>);

fn setup(mut commands: Commands) {
    commands.spawn().insert(BonusTimes::default());
}

fn bonus(
    mut commands: Commands,
    mut events: EventReader<AwardBonus>,
    mut bonus_times: Query<&mut BonusTimes>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    level: Query<(&Map, Entity)>,
) {
    for _ in events.iter() {
        if let Ok((_, map_entity)) = level.single() {
            if let Ok(mut robot_kill_times) = bonus_times.single_mut() {
                robot_kill_times.push(Instant::now());
                let buffer = buffers.get_handle(sfx.bonus);
                let recent_kills = (robot_kill_times.len() % 7) - 1;
                let notes = vec![0., 2., 4., 5., 7., 9., 11.];
                let pitch = 1. + notes[recent_kills] / 12.;
                let sound_id = commands
                    .spawn()
                    .insert(Sound {
                        buffer,
                        state: SoundState::Playing,
                        gain: 3.,
                        pitch,
                        ..Default::default()
                    })
                    .id();
                commands.entity(map_entity).push_children(&[sound_id]);
            }
        }
    }
}

fn bonus_clear(
    mut commands: Commands,
    mut bonus_times: Query<&mut BonusTimes>,
    buffers: Res<Assets<Buffer>>,
    sfx: Res<Sfx>,
    level: Query<(&Map, Entity)>,
    mut events: EventReader<Reset>,
) {
    if let Ok(mut robot_kill_times) = bonus_times.single_mut() {
        for _ in events.iter() {
            robot_kill_times.clear();
        }
        if robot_kill_times.is_empty() {
            return;
        }
        robot_kill_times.retain(|v| v.elapsed().as_secs() <= 10);
        if robot_kill_times.is_empty() {
            if let Ok((_, map_entity)) = level.single() {
                let buffer = buffers.get_handle(sfx.bonus_clear);
                let sound_id = commands
                    .spawn()
                    .insert(Sound {
                        buffer,
                        state: SoundState::Playing,
                        gain: 3.,
                        ..Default::default()
                    })
                    .id();
                commands.entity(map_entity).push_children(&[sound_id]);
            }
        }
    }
}

pub struct BonusPlugin;

impl Plugin for BonusPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_event::<AwardBonus>()
            .add_startup_system(setup.system())
            .add_system(bonus.system())
            .add_system(bonus_clear.system());
    }
}
