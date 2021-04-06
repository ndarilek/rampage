use bevy::prelude::*;
use bevy_input_actionmap::InputMap;
use bevy_tts::Tts;

#[bevy_main]
fn main() {
    App::build()
        .add_plugins(bevy_webgl2::DefaultPlugins)
        .add_plugin(bevy_input_actionmap::ActionPlugin::<String>::default())
        .add_plugin(bevy_tts::TtsPlugin)
        .add_startup_system(setup.system())
        .add_system(greet.system())
        .run();
}

const GREET: &str = "GREET";

fn setup(mut input: ResMut<InputMap<String>>) {
    input.bind(GREET, KeyCode::G);
}

fn greet(input: Res<InputMap<String>>, mut tts: ResMut<Tts>) {
    if input.just_active(GREET) {
        tts.speak("Hello, world.", true).unwrap();
    }
}
