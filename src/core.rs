use std::cmp::{max, min};

use bevy::{core::FloatOrd, prelude::*, transform::TransformSystem};
use derive_more::{Deref, DerefMut};

#[derive(Clone, Copy, Debug, Default, Deref, DerefMut, PartialEq, PartialOrd, Reflect)]
#[reflect(Component)]
pub struct Coordinates(pub (f32, f32));

impl From<(f32, f32)> for Coordinates {
    fn from(v: (f32, f32)) -> Self {
        Coordinates((v.0, v.1))
    }
}

impl From<(i32, i32)> for Coordinates {
    fn from(v: (i32, i32)) -> Self {
        Coordinates((v.0 as f32, v.1 as f32))
    }
}

impl From<(usize, usize)> for Coordinates {
    fn from(v: (usize, usize)) -> Self {
        Coordinates((v.0 as f32, v.1 as f32))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Area {
    pub rect: mapgen::geometry::Rect,
    pub description: Option<String>,
}

impl Area {
    pub fn contains(&self, point: &dyn PointLike) -> bool {
        let x = point.x() as usize;
        let y = point.y() as usize;
        x >= self.rect.x1 && x <= self.rect.x2 && y >= self.rect.y1 && y <= self.rect.y2
    }
}

#[derive(Clone, Copy, Debug, Reflect)]
pub enum Angle {
    Degrees(f32),
    Radians(f32),
}

impl Default for Angle {
    fn default() -> Self {
        Self::Radians(0.)
    }
}

impl Angle {
    pub fn degrees(&self) -> f32 {
        use Angle::*;
        let mut degrees: f32 = match self {
            Degrees(v) => *v,
            Radians(v) => v.to_degrees(),
        };
        while degrees < 0. {
            degrees += 360.;
        }
        while degrees >= 360. {
            degrees %= 360.;
        }
        degrees
    }

    pub fn degrees_u32(&self) -> u32 {
        self.degrees() as u32
    }

    pub fn radians(&self) -> f32 {
        use Angle::*;
        match self {
            Degrees(v) => v.to_radians(),
            Radians(v) => *v,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementDirection {
    North,
    NorthNortheast,
    Northeast,
    EastNortheast,
    East,
    EastSoutheast,
    Southeast,
    SouthSoutheast,
    South,
    SouthSouthwest,
    Southwest,
    WestSouthwest,
    West,
    WestNorthwest,
    Northwest,
    NorthNorthwest,
}

impl MovementDirection {
    pub fn new(heading: f32) -> Self {
        use MovementDirection::*;
        let mut heading = heading;
        while heading >= 360. {
            heading -= 360.;
        }
        while heading < 0. {
            heading += 360.;
        }
        match heading {
            h if h < 11.5 => East,
            h if h < 34.0 => EastNortheast,
            h if h < 56.5 => Northeast,
            h if h < 79.0 => NorthNortheast,
            h if h < 101.5 => North,
            h if h < 124.0 => NorthNorthwest,
            h if h < 146.5 => Northwest,
            h if h < 169.0 => WestNorthwest,
            h if h < 191.5 => West,
            h if h < 214.0 => WestSouthwest,
            h if h < 236.5 => Southwest,
            h if h < 259.0 => SouthSouthwest,
            h if h < 281.5 => South,
            h if h < 304.0 => SouthSoutheast,
            h if h < 326.5 => Southeast,
            h if h <= 349.0 => EastSoutheast,
            _ => East,
        }
    }
}

impl From<Angle> for MovementDirection {
    fn from(angle: Angle) -> Self {
        MovementDirection::new(angle.degrees())
    }
}

// Converting from strings into directions doesn't make sense.
#[allow(clippy::from_over_into)]
impl Into<String> for MovementDirection {
    fn into(self) -> String {
        use MovementDirection::*;
        match self {
            North => "north".to_string(),
            NorthNortheast => "north northeast".to_string(),
            Northeast => "northeast".to_string(),
            EastNortheast => "east northeast".to_string(),
            East => "east".to_string(),
            EastSoutheast => "east southeast".to_string(),
            Southeast => "southeast".to_string(),
            SouthSoutheast => "south southeast".to_string(),
            South => "south".to_string(),
            SouthSouthwest => "south southwest".to_string(),
            Southwest => "southwest".to_string(),
            WestSouthwest => "west southwest".to_string(),
            West => "west".to_string(),
            WestNorthwest => "west northwest".to_string(),
            Northwest => "northwest".to_string(),
            NorthNorthwest => "north northwest".to_string(),
        }
    }
}

pub trait PointLike {
    fn x(&self) -> f32;

    fn y(&self) -> f32;

    fn x_i32(&self) -> i32 {
        self.x() as i32
    }

    fn y_i32(&self) -> i32 {
        self.y() as i32
    }

    fn to_index(&self, width: usize) -> usize {
        ((self.y_i32() * width as i32) + self.x_i32()) as usize
    }

    fn distance_squared(&self, other: &dyn PointLike) -> f32 {
        let x1 = FloatOrd(self.x());
        let y1 = FloatOrd(self.y());
        let x2 = FloatOrd(other.x());
        let y2 = FloatOrd(other.y());
        let dx = max(x1, x2).0 - min(x1, x2).0;
        let dy = max(y1, y2).0 - min(y1, y2).0;
        (dx * dx) + (dy * dy)
    }

    fn distance(&self, other: &dyn PointLike) -> f32 {
        self.distance_squared(other).sqrt()
    }

    fn bearing(&self, other: &dyn PointLike) -> f32 {
        let y = other.y() - self.y();
        let x = other.x() - self.x();
        y.atan2(x)
    }

    fn direction(&self, other: &dyn PointLike) -> MovementDirection {
        let heading = self.bearing(other);
        MovementDirection::new(heading.to_degrees())
    }

    fn distance_and_direction(&self, other: &dyn PointLike) -> String {
        let mut tokens: Vec<String> = vec![];
        let distance = self.distance(other).round() as i32;
        if distance > 0 {
            let tile_or_tiles = if distance == 1 { "tile" } else { "tiles" };
            let direction: String = self.direction(other).into();
            tokens.push(format!("{} {} {}", distance, tile_or_tiles, direction));
        }
        tokens.join(" ")
    }
}

impl PointLike for (i32, i32) {
    fn x(&self) -> f32 {
        self.0 as f32
    }

    fn y(&self) -> f32 {
        self.1 as f32
    }
}

impl PointLike for (f32, f32) {
    fn x(&self) -> f32 {
        self.0
    }

    fn y(&self) -> f32 {
        self.1
    }
}

impl PointLike for (usize, usize) {
    fn x(&self) -> f32 {
        self.0 as f32
    }

    fn y(&self) -> f32 {
        self.1 as f32
    }
}

impl PointLike for &Coordinates {
    fn x(&self) -> f32 {
        self.0 .0
    }

    fn y(&self) -> f32 {
        self.0 .1
    }
}

#[macro_export]
macro_rules! impl_pointlike_for_tuple_component {
    ($source:ty) => {
        impl PointLike for $source {
            fn x(&self) -> f32 {
                self.0 .0 as f32
            }

            fn y(&self) -> f32 {
                self.0 .1 as f32
            }
        }
    };
}

impl_pointlike_for_tuple_component!(Coordinates);

impl From<&dyn PointLike> for (i32, i32) {
    fn from(val: &dyn PointLike) -> Self {
        (val.x_i32(), val.y_i32())
    }
}

#[derive(Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Player;

fn copy_coordinates_to_transform(
    mut query: Query<(&Coordinates, &mut Transform), Changed<Coordinates>>,
) {
    for (coordinates, mut transform) in query.iter_mut() {
        transform.translation.x = coordinates.0 .0;
        transform.translation.y = coordinates.0 .1;
    }
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.register_type::<Coordinates>()
            .add_system(copy_coordinates_to_transform.system())
            .add_system_to_stage(
                CoreStage::PostUpdate,
                copy_coordinates_to_transform
                    .system()
                    .before(TransformSystem::TransformPropagate),
            );
    }
}
