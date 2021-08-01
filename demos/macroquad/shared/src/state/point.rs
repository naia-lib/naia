use nanoserde::{DeBin, SerBin};

use naia_derive::Actor;
use naia_shared::{Actor, Property};

use super::State;

#[derive(Clone, PartialEq, DeBin, SerBin)]
pub enum Color {
    Red,
    Blue,
    Yellow,
}

impl Default for Color {
    fn default() -> Self {
        Color::Red
    }
}

#[derive(Actor)]
#[type_name = "State"]
pub struct Point {
    pub x: Property<u16>,
    pub y: Property<u16>,
    pub color: Property<Color>,
}

impl Point {
    pub fn new(x: u16, y: u16, color: Color) -> Point {
        return Point::new_complete(x, y, color);
    }
}
