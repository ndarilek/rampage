use bevy::prelude::*;
use bevy_tts::Tts;

#[bevy_main]
fn main() {
    App::build()
        .add_plugins(DefaultPlugins)
        .add_plugin(bevy_tts::TtsPlugin)
        .add_startup_system(setup.system())
        .run();
}

fn setup(mut tts: ResMut<Tts>) {
    tts.speak("Hello, world.", true).unwrap();
}
