use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use coord_2d::{Coord, Size};
use derive_more::{Deref, DerefMut};
use shadowcast::{vision_distance, Context, InputGrid};

use crate::{
    core::{Coordinates, Player, PointLike},
    log::Log,
    map::{ITileType, Map, MapConfig},
};

#[derive(Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct BlocksVisibility;

#[derive(Clone, Debug, Default, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct RevealedTiles(pub Vec<bool>);

#[derive(Clone, Debug)]
pub struct Viewshed {
    pub visible: HashSet<(i32, i32)>,
    pub range: u32,
}

impl Default for Viewshed {
    fn default() -> Self {
        Self {
            range: 15,
            visible: HashSet::new(),
        }
    }
}

#[allow(dead_code)]
impl Viewshed {
    pub fn is_visible(&self, point: &dyn PointLike) -> bool {
        self.visible.contains(&point.into())
    }
}

#[derive(Clone, Debug, Default, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct VisibilityBlocked(pub Vec<bool>);

#[derive(Clone, Debug, Default, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct VisibleTiles(pub Vec<bool>);

fn add_visibility_indices(
    mut commands: Commands,
    query: Query<
        (Entity, &Map),
        (
            Added<Map>,
            Without<VisibilityBlocked>,
            Without<VisibleTiles>,
            Without<RevealedTiles>,
        ),
    >,
    map_config: Res<MapConfig>,
) {
    for (entity, map) in query.iter() {
        let mut v = vec![];
        for tile in &map.base.tiles {
            v.push(tile.blocks_visibility());
        }
        commands.entity(entity).insert(VisibilityBlocked(v));
        let count = map.count();
        commands
            .entity(entity)
            .insert(VisibleTiles(vec![false; count]));
        commands
            .entity(entity)
            .insert(RevealedTiles(vec![map_config.start_revealed; count]));
    }
}

#[derive(Default, Deref, DerefMut)]
struct PreviousIndex(HashMap<Entity, usize>);

fn map_visibility_indexing(
    mut map: Query<(&Map, &mut VisibilityBlocked)>,
    mut prev_index: ResMut<PreviousIndex>,
    query: Query<
        (Entity, &Coordinates, &BlocksVisibility),
        Or<(Changed<Coordinates>, Changed<BlocksVisibility>)>,
    >,
    visibility_blockers: Query<&BlocksVisibility>,
) {
    for (entity, coordinates, _) in query.iter() {
        for (map, mut visibility_blocked) in map.iter_mut() {
            let idx = coordinates.to_index(map.width());
            if let Some(prev_idx) = prev_index.get(&entity) {
                if *prev_idx == idx {
                    continue;
                }
                let tile = map.base.tiles[*prev_idx];
                let mut new_visibility_blocked = tile.blocks_visibility();
                if !new_visibility_blocked {
                    for e in &map.entities[*prev_idx] {
                        if visibility_blockers.get(*e).is_ok() {
                            new_visibility_blocked = true;
                            break;
                        }
                    }
                }
                visibility_blocked[*prev_idx] = new_visibility_blocked;
            }
            visibility_blocked[idx] = true;
            prev_index.insert(entity, idx);
        }
    }
}

fn remove_blocks_visibility(
    mut prev_index: ResMut<PreviousIndex>,
    mut map: Query<(&Map, &mut VisibilityBlocked)>,
    removed: RemovedComponents<BlocksVisibility>,
    coordinates: Query<&Coordinates>,
    blocks_visibility: Query<&BlocksVisibility>,
) {
    for entity in removed.iter() {
        if let Ok(coordinates) = coordinates.get_component::<Coordinates>(entity) {
            prev_index.remove(&entity);
            for (map, mut visibility_blocked) in map.iter_mut() {
                let idx = (**coordinates).to_index(map.width());
                let tile = map.base.tiles[idx];
                let mut new_visibility_blocked = tile.blocks_visibility();
                for e in &map.entities[idx] {
                    new_visibility_blocked = new_visibility_blocked
                        || blocks_visibility
                            .get_component::<BlocksVisibility>(*e)
                            .is_ok();
                }
                visibility_blocked[idx] = new_visibility_blocked;
            }
        }
    }
}

struct VisibilityGrid(Map, VisibilityBlocked);

impl InputGrid for VisibilityGrid {
    type Grid = VisibilityGrid;

    type Opacity = u8;

    fn size(&self, grid: &Self::Grid) -> Size {
        Size::new(grid.0.width() as u32, grid.0.height() as u32)
    }

    fn get_opacity(&self, grid: &Self::Grid, coord: Coord) -> Self::Opacity {
        let point = (coord.x, coord.y);
        let index = point.to_index(grid.0.width());
        if grid.1 .0[index] {
            255
        } else {
            0
        }
    }
}

fn update_viewshed(
    mut viewers: Query<
        (&mut Viewshed, &Coordinates),
        Or<(Changed<VisibilityBlocked>, Changed<Coordinates>)>,
    >,
    map: Query<(&Map, &VisibilityBlocked)>,
) {
    for (mut viewshed, start) in viewers.iter_mut() {
        for (map, visibility_blocked) in map.iter() {
            let mut context: Context<u8> = Context::default();
            let vision_distance = vision_distance::Circle::new(viewshed.range);
            let coord = Coord::new(start.x_i32(), start.y_i32());
            viewshed.visible.clear();
            let visibility_grid = VisibilityGrid(map.clone(), visibility_blocked.clone());
            context.for_each_visible(
                coord,
                &visibility_grid,
                &visibility_grid,
                vision_distance,
                255,
                |coord, _directions, _visibility| {
                    viewshed.visible.insert((coord.x, coord.y));
                },
            );
        }
    }
}

fn map_visibility(
    mut map: Query<
        (
            &Map,
            &VisibilityBlocked,
            &mut RevealedTiles,
            &mut VisibleTiles,
        ),
        Or<(Changed<Map>, Changed<VisibilityBlocked>)>,
    >,
    viewers: Query<(&Player, &Viewshed)>,
) {
    for (_, viewshed) in viewers.iter() {
        for (map, _, mut revealed_tiles, mut visible_tiles) in map.iter_mut() {
            for t in visible_tiles.iter_mut() {
                *t = false
            }
            for v in viewshed.visible.iter() {
                let idx = (*v).to_index(map.width());
                revealed_tiles[idx] = true;
                visible_tiles[idx] = true;
            }
        }
    }
}

fn log_visible(
    time: Res<Time>,
    mut seen: Local<HashSet<Entity>>,
    mut recently_lost: Local<HashMap<Entity, Timer>>,
    mut log: Query<&mut Log>,
    viewers: Query<(&Viewshed, &Coordinates, &Player)>,
    map: Query<&Map>,
    names: Query<&Name>,
    players: Query<&Player>,
) {
    for timer in recently_lost.values_mut() {
        timer.tick(time.delta());
    }
    let recently_lost_clone = recently_lost.clone();
    for (entity, timer) in recently_lost_clone.iter() {
        if timer.finished() {
            recently_lost.remove(&entity);
        }
    }
    let mut new_seen = HashSet::new();
    if let Ok(mut log) = log.single_mut() {
        for (viewshed, coordinates, _) in viewers.iter() {
            for viewed_coordinates in &viewshed.visible {
                for map in map.iter() {
                    let index = viewed_coordinates.to_index(map.width());
                    for entity in &map.entities[index] {
                        if recently_lost.contains_key(&entity) {
                            continue;
                        }
                        if let Ok(name) = names.get(*entity) {
                            if players.get(*entity).is_err() {
                                if !seen.contains(&*entity) {
                                    let name = name.to_string();
                                    let location =
                                        coordinates.distance_and_direction(viewed_coordinates);
                                    log.push(format!("{}: {}", name, location));
                                }
                                new_seen.insert(*entity);
                            }
                        }
                    }
                }
            }
        }
    }
    let recently_lost_entities = seen.difference(&new_seen);
    for entity in recently_lost_entities {
        recently_lost.insert(*entity, Timer::from_seconds(2., false));
    }
    *seen = new_seen;
}

pub const LOG_VISIBLE_LABEL: &str = "LOG_VISIBLE";

pub struct VisibilityPlugin;

impl Plugin for VisibilityPlugin {
    fn build(&self, app: &mut AppBuilder) {
        const UPDATE_VISIBILITY_INDEX: &str = "UPDATE_VISIBILITY_INDEX";
        const UPDATE_VIEWSHED: &str = "UPDATE_VIEWSHED";
        const MAP_VISIBILITY: &str = "MAP_VISIBILITY";
        app.insert_resource(PreviousIndex::default())
            .add_system(add_visibility_indices.system())
            .add_system_to_stage(
                CoreStage::PostUpdate,
                add_visibility_indices
                    .system()
                    .before(UPDATE_VISIBILITY_INDEX),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                remove_blocks_visibility
                    .system()
                    .before(UPDATE_VISIBILITY_INDEX),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                map_visibility_indexing
                    .system()
                    .label(UPDATE_VISIBILITY_INDEX)
                    .after(crate::map::UPDATE_ENTITY_INDEX_LABEL),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                update_viewshed
                    .system()
                    .label(UPDATE_VIEWSHED)
                    .after(UPDATE_VISIBILITY_INDEX),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                map_visibility
                    .system()
                    .label(MAP_VISIBILITY)
                    .after(UPDATE_VIEWSHED),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                log_visible
                    .system()
                    .label(LOG_VISIBLE_LABEL)
                    .after(MAP_VISIBILITY),
            );
    }
}
