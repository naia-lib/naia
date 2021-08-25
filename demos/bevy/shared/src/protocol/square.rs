use nanoserde::{DeBin, SerBin};

use naia_derive::Replicate;
use naia_shared::Property;

use super::Protocol;

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

#[derive(Replicate, Clone)]
pub struct Square {
    pub x: Property<i16>,
    pub y: Property<i16>,
    pub color: Property<Color>,
}

impl Square {
    pub fn new(x: i16, y: i16, color: Color) -> Ref<Square> {
        return Square::new_complete(x, y, color);
    }
}
