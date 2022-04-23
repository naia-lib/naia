use naia_shared::{derive_serde, serde, Property, Replicate};

#[derive_serde]
pub enum Color {
    Red,
    Blue,
    Yellow,
    Green,
}

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
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
