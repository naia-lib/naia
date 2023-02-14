use naia_shared::{Property, Replicate, Serde};

#[derive(Clone, PartialEq, Serde)]
pub enum Color {
    Red,
    Blue,
    Yellow,
    Green,
}

#[derive(Replicate)]
pub struct Square {
    pub x: Property<u16>,
    pub y: Property<u16>,
    pub color: Property<Color>,
}

impl Square {
    pub fn new(x: u16, y: u16, color: Color) -> Self {
        Square::new_complete(x, y, color)
    }
}
