use nanoserde::{DeBin, SerBin};

use naia_derive::Replicate;
use naia_shared::{Property, Replicate};

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
    pub x: Property<u16>,
    pub y: Property<u16>,
    pub color: Property<Color>,
}

impl Square {
    pub fn new(x: u16, y: u16, color: Color) -> Square {
        return Square::new_complete(x, y, color);
    }
}
