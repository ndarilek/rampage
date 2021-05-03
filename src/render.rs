use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;
use blackout::{map::Map as BlackoutMap, mapgen::TileType};

use crate::game::AssetHandles;

fn spawn_tilemap(
    mut commands: Commands,
    handles: Res<AssetHandles>,
    materials: Res<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    map: Query<(Entity, &BlackoutMap), Added<BlackoutMap>>,
) {
    for (entity, map) in map.iter() {
        let material_handle = materials.get_handle(handles.tiles.clone());
        let mut tilemap = Map::new(
            Vec2::new(1., 1.).into(),
            Vec2::new(map.width() as f32, map.height() as f32).into(),
            Vec2::new(16., 16.),
            Vec2::new(160., 16.),
            0,
        );
        let map_entity = commands.spawn().id();
        tilemap.build(
            &mut commands,
            &mut meshes,
            material_handle.clone(),
            map_entity,
            false,
        );
        for y in 0..map.height() {
            for x in 0..map.width() {
                let position = Vec2::new(x as f32, y as f32);
                let tile_type = map.base.at(x, y);
                tilemap
                    .add_tile(
                        &mut commands,
                        position.into(),
                        Tile {
                            texture_index: match tile_type {
                                TileType::Wall => 6,
                                TileType::Floor => 8,
                            },
                            ..Default::default()
                        },
                    )
                    .expect("Failed to create tilemap");
            }
        }
        commands.entity(map_entity).insert_bundle(MapBundle {
            map: tilemap,
            ..Default::default()
        });
        commands.entity(entity).push_children(&[map_entity]);
    }
}

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(TileMapPlugin)
            .add_system(spawn_tilemap.system());
    }
}
