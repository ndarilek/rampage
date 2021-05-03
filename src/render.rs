use bevy::prelude::*;
use blackout::map::Map;

fn spawn_tilemap(map: Query<&Map, Added<Map>>) {
    for _map in map.iter() {
        //println!("Should spawn tilemap");
    }
}

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_system(spawn_tilemap.system());
    }
}
