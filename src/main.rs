use std::error::Error;

use bevy::prelude::*;
use bevy_input_actionmap::InputMap;
use bevy_tts::Tts;

mod error;

use crate::error::error_handler;

#[bevy_main]
fn main() {
    App::build()
        .add_plugins(DefaultPlugins)
        .add_plugin(crate::error::ErrorPlugin)
        .add_plugin(bevy_input_actionmap::ActionPlugin::<String>::default())
        .add_plugin(bevy_tts::TtsPlugin)
        .add_startup_system(setup.system())
        .add_system(greet.system().chain(error_handler.system()))
        .run();
}

const GREET: &str = "GREET";

fn setup(mut input: ResMut<InputMap<String>>) {
    input.bind(GREET, KeyCode::G);
}

fn greet(input: Res<InputMap<String>>, mut tts: ResMut<Tts>) -> Result<(), Box<dyn Error>> {
    if input.just_active(GREET) {
        tts.speak("Hello, world.", true)?;
    }
    Ok(())
}
