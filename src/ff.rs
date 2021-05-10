use bevy::{app::Events, prelude::*};
use blackout::gilrs::{
    ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks},
    GamepadId, Gilrs,
};

use crate::player::Shoot;

fn setup(mut commands: Commands, gilrs: NonSend<Gilrs>) {
    let mut support_ff = Vec::new();
    for (id, gamepad) in gilrs.gamepads() {
        let ff = gamepad.is_ff_supported();
        if ff {
            support_ff.push(id);
        }
    }
    commands.insert_resource(support_ff);
}

fn generate_ff(world: &mut World) {
    let world = world.cell();
    let mut gilrs = world.get_non_send_mut::<Gilrs>().unwrap();
    let support_ff = world.get_resource::<Vec<GamepadId>>().unwrap();
    if !support_ff.is_empty() {
        if let Some(events) = world.get_resource::<Events<Shoot>>() {
            let mut reader = events.get_reader();
            for _ in reader.iter(&events) {
                let effect = EffectBuilder::new()
                    .add_effect(BaseEffect {
                        kind: BaseEffectType::Strong { magnitude: 60_000 },
                        scheduling: Replay {
                            play_for: Ticks::from_ms(50),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .gamepads(&*support_ff)
                    .finish(&mut gilrs)
                    .unwrap();
                effect.play().unwrap();
            }
        }
    }
}

pub struct ForceFeedbackPlugin;

impl Plugin for ForceFeedbackPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_startup_system(setup.system())
            .add_system_to_stage(CoreStage::PostUpdate, generate_ff.exclusive_system());
    }
}
