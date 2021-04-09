use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use derive_more::{Deref, DerefMut};
use mapgen::{geometry::Rect as MRect, Map as MapgenMap, MapFilter, TileType};
use maze_generator::{prelude::*, recursive_backtracking::RbGenerator};
use rand::prelude::StdRng;

use crate::{
    core::{Area, Coordinates, Player, PointLike},
    log::Log,
    navigation::MonitorsCollisions,
};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Areas(pub Vec<Area>);

#[derive(Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Exit;

#[derive(Clone, Default)]
pub struct Map {
    pub base: MapgenMap,
    pub entities: Vec<HashSet<Entity>>,
}

impl Map {
    pub fn new(base: MapgenMap) -> Self {
        let count = (base.width * base.height) as usize;
        Self {
            base,
            entities: vec![HashSet::new(); count],
        }
    }

    pub fn width(&self) -> usize {
        self.base.width
    }

    pub fn height(&self) -> usize {
        self.base.height
    }

    pub fn count(&self) -> usize {
        self.width() * self.height()
    }

    pub fn start(&self) -> Option<mapgen::geometry::Point> {
        self.base.starting_point
    }

    pub fn exit(&self) -> Option<mapgen::geometry::Point> {
        self.base.exit_point
    }
}

pub trait ITileType {
    fn blocks_motion(&self) -> bool;
    fn blocks_visibility(&self) -> bool;
}

impl ITileType for TileType {
    fn blocks_motion(&self) -> bool {
        match self {
            TileType::Wall => true,
            TileType::Floor => false,
        }
    }

    fn blocks_visibility(&self) -> bool {
        match self {
            TileType::Wall => true,
            TileType::Floor => false,
        }
    }
}

struct MapConfig {
    autospawn_exits: bool,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            autospawn_exits: true,
        }
    }
}

#[derive(Bundle)]
pub struct ExitBundle {
    pub coordinates: Coordinates,
    pub monitors_collisions: MonitorsCollisions,
    pub exit: Exit,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
}

impl Default for ExitBundle {
    fn default() -> Self {
        Self {
            coordinates: Default::default(),
            monitors_collisions: Default::default(),
            exit: Default::default(),
            transform: Default::default(),
            global_transform: Default::default(),
        }
    }
}

pub struct GridBuilder;

impl GridBuilder {
    pub fn new() -> Box<GridBuilder> {
        Box::new(GridBuilder {})
    }
}

impl MapFilter for GridBuilder {
    fn modify_map(&self, _rng: &mut StdRng, map: &MapgenMap) -> MapgenMap {
        let mut map = map.clone();
        let mut generator = RbGenerator::new(None);
        let maze = generator.generate(10, 10);
        for y in 0..=9 {
            for x in 0..=9 {
                let x_offset = x * 10;
                let y_offset = 90 - y * 10;
                let room = MRect::new_i32(x_offset + 1, y_offset + 1, 9, 9);
                map.add_room(room);
                let coords = maze_generator::prelude::Coordinates::new(x, y);
                if let Some(field) = maze.get_field(&coords) {
                    use maze_generator::prelude::Direction::*;
                    if field.has_passage(&North) {
                        let x = x_offset + 5;
                        let y = y_offset + 10;
                        map.set_tile(x as usize, y as usize, TileType::Floor);
                    }
                    if field.has_passage(&South) {
                        let x = x_offset + 5;
                        let y = y_offset;
                        map.set_tile(x as usize, y as usize, TileType::Floor);
                    }
                    if field.has_passage(&East) {
                        let x = x_offset + 10;
                        let y = y_offset + 5;
                        map.set_tile(x as usize, y as usize, TileType::Floor);
                    }
                    if field.has_passage(&West) {
                        let x = x_offset;
                        let y = y_offset + 5;
                        map.set_tile(x as usize, y as usize, TileType::Floor);
                    }
                }
            }
        }
        map
    }
}

fn exit_spawner(mut commands: Commands, map: Query<&Map, Added<Map>>, config: Res<MapConfig>) {
    for map in map.iter() {
        if config.autospawn_exits {
            let mut exits: Vec<(f32, f32)> = vec![];
            for x in 1..map.width() {
                for y in 1..map.height() {
                    let mut spawn_exit = false;
                    if map.base.get_available_exits(x, y).len() > 2 {
                        let idx = (x, y).to_index(map.width());
                        if map.base.tiles[idx] == TileType::Floor
                            && (x > 1 && map.base.tiles[idx - 1] == TileType::Floor)
                            && (x < map.width() - 2 && map.base.tiles[idx + 1] == TileType::Floor)
                            && (y > 1
                                && map.base.tiles[idx - map.width() as usize] == TileType::Wall)
                            && (y < map.height() - 2
                                && map.base.tiles[idx + map.width() as usize] == TileType::Wall)
                        {
                            spawn_exit = true;
                        }
                        if map.base.tiles[idx] == TileType::Floor
                            && (x > 1 && map.base.tiles[idx - 1] == TileType::Wall)
                            && (x < map.width() - 2 && map.base.tiles[idx + 1] == TileType::Wall)
                            && (y > 1
                                && map.base.tiles[idx - map.width() as usize] == TileType::Floor)
                            && (y < map.height() - 2
                                && map.base.tiles[idx + map.width() as usize] == TileType::Floor)
                        {
                            spawn_exit = true;
                        }
                    }
                    if spawn_exit {
                        let x = x as f32;
                        let y = y as f32;
                        if !exits.contains(&(x, y)) {
                            exits.push((x, y));
                        }
                    }
                }
            }
            for exit in exits {
                let x = exit.0 as f32;
                let y = exit.1 as f32;
                commands.spawn().insert_bundle(ExitBundle {
                    coordinates: Coordinates((x, y)),
                    exit: Default::default(),
                    transform: Transform::from_translation(Vec3::new(x, y, 0.)),
                    ..Default::default()
                });
            }
        }
    }
}

fn area_description(
    mut prev_area: Local<Option<Area>>,
    query: Query<(&Player, &Coordinates), Changed<Coordinates>>,
    map: Query<(&Map, &Areas)>,
    mut log: Query<&mut Log>,
) {
    for (_, coordinates) in query.iter() {
        for (_, areas) in map.iter() {
            let mut should_describe_area = false;
            let mut current_area: Option<Area> = None;
            for area in areas.iter() {
                if area.contains(&*coordinates) {
                    current_area = Some(area.clone());
                    if let Some(prev_area) = &*prev_area {
                        if prev_area != area {
                            should_describe_area = true;
                        }
                    } else {
                        should_describe_area = true;
                    }
                    break;
                }
            }
            if should_describe_area {
                if let Some(ref area) = current_area {
                    let description = if area.description.is_some() {
                        area.description.as_ref().unwrap().clone()
                    } else {
                        format!("{} by {} area.", area.rect.width(), area.rect.height())
                    };
                    for mut log in log.iter_mut() {
                        log.push(description.clone());
                    }
                }
            }
            *prev_area = current_area;
        }
    }
}

#[derive(Default, Deref, DerefMut)]
struct PreviousIndex(HashMap<Entity, usize>);

fn entity_indexing(
    mut map: Query<&mut Map>,
    mut previous_index: ResMut<PreviousIndex>,
    query: Query<(Entity, &Coordinates), Changed<Coordinates>>,
) {
    for (entity, coordinates) in query.iter() {
        for mut map in map.iter_mut() {
            let idx = coordinates.to_index(map.width());
            if let Some(prev_idx) = previous_index.get(&entity) {
                if idx != *prev_idx {
                    map.entities[*prev_idx].retain(|&e| e != entity);
                }
            }
            map.entities[idx].insert(entity);
            previous_index.insert(entity, idx);
        }
    }
}

fn add_areas(mut commands: Commands, query: Query<(Entity, &Map), (Added<Map>, Without<Areas>)>) {
    for (entity, map) in query.iter() {
        let mut v = vec![];
        for room in &map.base.rooms {
            v.push(Area {
                rect: *room,
                description: None,
            });
        }
        commands.entity(entity).insert(Areas(v));
    }
}

pub const UPDATE_ENTITY_INDEX_LABEL: &str = "UPDATE_ENTITY_INDEX";

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut AppBuilder) {
        if !app.world().contains_resource::<MapConfig>() {
            app.insert_resource(MapConfig::default());
        }
        const SPAWN_EXITS: &str = "SPAWN_EXITS";
        app.register_type::<Exit>()
            .insert_resource(PreviousIndex::default())
            .add_system(entity_indexing.system().label(UPDATE_ENTITY_INDEX_LABEL))
            .add_system(
                exit_spawner
                    .system()
                    .label(SPAWN_EXITS)
                    .before(UPDATE_ENTITY_INDEX_LABEL),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                entity_indexing.system().label(UPDATE_ENTITY_INDEX_LABEL),
            )
            .add_system_to_stage(CoreStage::PostUpdate, area_description.system())
            .add_system_to_stage(CoreStage::Update, add_areas.system())
            .add_system_to_stage(CoreStage::PostUpdate, add_areas.system());
    }
}