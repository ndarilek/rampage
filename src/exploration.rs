use std::error::Error;

use bevy::prelude::*;
use bevy_input_actionmap::InputMap;
use bevy_tts::Tts;
use derive_more::{Deref, DerefMut};
use mapgen::TileType;

use crate::{
    core::{Coordinates, Player, PointLike},
    error::error_handler,
    map::Map,
    pathfinding::Destination,
    visibility::{RevealedTiles, Viewshed, VisibleTiles},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct ExplorationFocused;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Reflect)]
pub enum ExplorationType {
    Exit = 0,
    Item = 1,
    Character = 2,
    Ally = 3,
    Enemy = 4,
}

// Doesn't make sense to create from a `String`.
#[allow(clippy::from_over_into)]
impl Into<String> for ExplorationType {
    fn into(self) -> String {
        match self {
            ExplorationType::Exit => "Exit".into(),
            ExplorationType::Item => "Item".into(),
            ExplorationType::Character => "Character".into(),
            ExplorationType::Ally => "Ally".into(),
            ExplorationType::Enemy => "Enemy".into(),
        }
    }
}

// Likewise.
#[allow(clippy::from_over_into)]
impl Into<&str> for ExplorationType {
    fn into(self) -> &'static str {
        match self {
            ExplorationType::Exit => "exit",
            ExplorationType::Item => "item",
            ExplorationType::Character => "character",
            ExplorationType::Ally => "ally",
            ExplorationType::Enemy => "enemy",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut, Reflect)]
#[reflect(Component)]
pub struct Exploring(pub (f32, f32));

impl_pointlike_for_tuple_component!(Exploring);

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct FocusedExplorationType(pub Option<ExplorationType>);

#[derive(Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Mappable;

pub const ACTION_EXPLORE_FORWARD: &str = "explore_forward";
pub const ACTION_EXPLORE_BACKWARD: &str = "explore_backward";
pub const ACTION_EXPLORE_LEFT: &str = "explore_left";
pub const ACTION_EXPLORE_RIGHT: &str = "explore_right";
pub const ACTION_EXPLORE_FOCUS_NEXT: &str = "explore_focus_next";
pub const ACTION_EXPLORE_FOCUS_PREV: &str = "explore_focus_prev";
pub const ACTION_EXPLORE_SELECT_NEXT_TYPE: &str = "explore_select_next_type";
pub const ACTION_EXPLORE_SELECT_PREV_TYPE: &str = "explore_select_prev_type";
pub const ACTION_NAVIGATE_TO_EXPLORED: &str = "navigate_to";

fn exploration_type_change(
    mut tts: ResMut<Tts>,
    input: Res<InputMap<String>>,
    mut explorers: Query<(&Player, &Viewshed, &mut FocusedExplorationType)>,
    features: Query<(&Coordinates, &ExplorationType)>,
) -> Result<(), Box<dyn Error>> {
    let changed = input.just_active(ACTION_EXPLORE_SELECT_NEXT_TYPE)
        || input.just_active(ACTION_EXPLORE_SELECT_PREV_TYPE);
    if !changed {
        return Ok(());
    }
    for (_, viewshed, mut focused) in explorers.iter_mut() {
        let mut types: Vec<ExplorationType> = vec![];
        for (coordinates, t) in features.iter() {
            let (x, y) = **coordinates;
            let x = x as i32;
            let y = y as i32;
            if viewshed.visible.contains(&(x, y)) {
                types.push(*t);
            }
        }
        types.sort();
        types.dedup();
        if types.is_empty() {
            tts.speak("Nothing visible.", true)?;
        } else if input.just_active(ACTION_EXPLORE_SELECT_PREV_TYPE) {
            if let Some(t) = &focused.0 {
                if let Some(i) = types.iter().position(|v| *v == *t) {
                    if i == 0 {
                        focused.0 = None;
                    } else {
                        let t = &types[i - 1];
                        focused.0 = Some(*t);
                    }
                } else {
                    let t = types.last().unwrap();
                    focused.0 = Some(*t);
                }
            } else {
                let t = types.last().unwrap();
                focused.0 = Some(*t);
            }
        } else if input.just_active(ACTION_EXPLORE_SELECT_NEXT_TYPE) {
            if let Some(t) = &focused.0 {
                if let Some(i) = types.iter().position(|v| *v == *t) {
                    if i == types.len() - 1 {
                        focused.0 = None;
                    } else {
                        let t = &types[i + 1];
                        focused.0 = Some(*t);
                    }
                } else {
                    let t = types.first().unwrap();
                    focused.0 = Some(*t);
                }
            } else {
                let t = types.first().unwrap();
                focused.0 = Some(*t)
            }
        }
    }
    Ok(())
}

fn exploration_type_focus(
    mut commands: Commands,
    input: Res<InputMap<String>>,
    mut tts: ResMut<Tts>,
    explorers: Query<(
        Entity,
        &Player,
        &Viewshed,
        &FocusedExplorationType,
        Option<&Exploring>,
    )>,
    features: Query<(&Coordinates, &ExplorationType)>,
) -> Result<(), Box<dyn Error>> {
    let changed = input.just_active(ACTION_EXPLORE_FOCUS_NEXT)
        || input.just_active(ACTION_EXPLORE_FOCUS_PREV);
    if !changed {
        return Ok(());
    }
    for (entity, _, viewshed, focused, exploring) in explorers.iter() {
        let mut features = features
            .iter()
            .filter(|(coordinates, _)| {
                let (x, y) = ***coordinates;
                let x = x as i32;
                let y = y as i32;
                viewshed.visible.contains(&(x, y))
            })
            .collect::<Vec<(&Coordinates, &ExplorationType)>>();
        features.sort_by(|(c1, _), (c2, _)| c1.partial_cmp(c2).unwrap());
        if let Some(focused) = &focused.0 {
            features.retain(|(_, t)| **t == *focused);
        }
        if features.is_empty() {
            tts.speak("Nothing visible.", true)?;
        } else {
            let mut target: Option<&(&Coordinates, &ExplorationType)> = None;
            if input.just_active(ACTION_EXPLORE_FOCUS_NEXT) {
                if let Some(exploring) = exploring {
                    target = features.iter().find(|(c, _)| ***c > **exploring);
                    if target.is_none() {
                        target = features.first();
                    }
                } else {
                    target = features.first();
                }
            } else if input.just_active(ACTION_EXPLORE_FOCUS_PREV) {
                if let Some(exploring) = exploring {
                    features.reverse();
                    target = features.iter().find(|(c, _)| ***c < **exploring);
                    if target.is_none() {
                        target = features.first();
                    }
                } else {
                    target = features.last();
                }
            }
            if let Some((coordinates, _)) = target {
                commands.entity(entity).insert(Exploring(***coordinates));
            }
        }
    }
    Ok(())
}

fn exploration_type_changed_announcement(
    mut tts: ResMut<Tts>,
    focused: Query<
        (
            &FocusedExplorationType,
            ChangeTrackers<FocusedExplorationType>,
        ),
        Changed<FocusedExplorationType>,
    >,
) -> Result<(), Box<dyn Error>> {
    for (focused, changed) in focused.iter() {
        if changed.is_added() {
            return Ok(());
        }
        match &focused.0 {
            Some(v) => {
                let v: String = (*v).into();
                tts.speak(v, true)?;
            }
            None => {
                tts.speak("Everything", true)?;
            }
        };
    }
    Ok(())
}

fn exploration_focus(
    mut commands: Commands,
    input: Res<InputMap<String>>,
    map: Query<&Map>,
    explorers: Query<(Entity, &Player, &Coordinates, Option<&Exploring>)>,
) {
    for map in map.iter() {
        for (entity, _, coordinates, exploring) in explorers.iter() {
            let coordinates = **coordinates;
            let coordinates = (coordinates.0.floor(), coordinates.1.floor());
            let mut exploring = if let Some(exploring) = exploring {
                **exploring
            } else {
                coordinates
            };
            let orig = exploring;
            if input.just_active(ACTION_EXPLORE_FORWARD) {
                exploring.1 += 1.;
            } else if input.just_active(ACTION_EXPLORE_BACKWARD) {
                exploring.1 -= 1.;
            } else if input.just_active(ACTION_EXPLORE_LEFT) {
                exploring.0 -= 1.;
            } else if input.just_active(ACTION_EXPLORE_RIGHT) {
                exploring.0 += 1.;
            }
            if orig != exploring
                && exploring.0 >= 0.
                && exploring.0 < map.width() as f32
                && exploring.1 >= 0.
                && exploring.1 < map.height() as f32
            {
                commands.entity(entity).insert(Exploring(exploring));
            }
        }
    }
}

fn navigate_to_explored(
    mut commands: Commands,
    input: Res<InputMap<String>>,
    map: Query<(&Map, &RevealedTiles)>,
    explorers: Query<(Entity, &Exploring)>,
) {
    for (entity, exploring) in explorers.iter() {
        for (map, revealed_tiles) in map.iter() {
            let point = **exploring;
            let idx = point.to_index(map.width());
            let known = revealed_tiles[idx];
            if input.just_active(ACTION_NAVIGATE_TO_EXPLORED) && known {
                commands
                    .entity(entity)
                    .insert(Destination((point.x_i32(), point.y_i32())));
            }
        }
    }
}

fn exploration_changed_announcement(
    mut commands: Commands,
    mut tts: ResMut<Tts>,
    map: Query<(&Map, &RevealedTiles, &VisibleTiles)>,
    explorers: Query<(&Coordinates, &Exploring), Changed<Exploring>>,
    focused: Query<(Entity, &ExplorationFocused)>,
    names: Query<&Name>,
    types: Query<&ExplorationType>,
    mappables: Query<&Mappable>,
) -> Result<(), Box<dyn Error>> {
    for (coordinates, exploring) in explorers.iter() {
        let coordinates = **coordinates;
        let coordinates = (coordinates.0.floor(), coordinates.1.floor());
        for (map, revealed_tiles, visible_tiles) in map.iter() {
            let point = **exploring;
            let idx = point.to_index(map.width());
            let known = revealed_tiles[idx];
            let visible = visible_tiles[idx];
            let fog_of_war = known && !visible;
            let description = if known {
                let mut tokens: Vec<&str> = vec![];
                for (entity, _) in focused.iter() {
                    commands.entity(entity).remove::<ExplorationFocused>();
                }
                for entity in &map.entities[idx] {
                    commands
                        .entity(*entity)
                        .insert(ExplorationFocused::default());
                    if visible || mappables.get(*entity).is_ok() {
                        if let Ok(name) = names.get(*entity) {
                            tokens.push(name.as_str());
                        }
                        if tokens.is_empty() {
                            if let Ok(t) = types.get(*entity) {
                                tokens.push((*t).into());
                            }
                        }
                    }
                }
                if tokens.is_empty() {
                    match map.base.tiles[idx] {
                        TileType::Floor => "Floor".to_string(),
                        TileType::Wall => "Wall".to_string(),
                    }
                } else {
                    tokens.join(": ")
                }
            } else {
                "Unknown".to_string()
            };
            let mut tokens: Vec<String> = vec![coordinates.distance_and_direction(exploring)];
            if fog_of_war {
                tokens.push("in the fog of war".into());
            }
            tts.speak(format!("{}: {}", description, tokens.join(", ")), true)?;
        }
    }
    Ok(())
}

pub struct ExplorationPlugin;

impl Plugin for ExplorationPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.register_type::<ExplorationFocused>()
            .register_type::<ExplorationType>()
            .register_type::<Mappable>()
            .add_system(exploration_focus.system())
            .add_system(
                exploration_type_focus
                    .system()
                    .chain(error_handler.system()),
            )
            .add_system(
                exploration_type_change
                    .system()
                    .chain(error_handler.system()),
            )
            .add_system(navigate_to_explored.system())
            .add_system_to_stage(
                CoreStage::PostUpdate,
                exploration_type_changed_announcement
                    .system()
                    .chain(error_handler.system()),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                exploration_changed_announcement
                    .system()
                    .chain(error_handler.system()),
            );
    }
}
