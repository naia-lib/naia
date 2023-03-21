use naia_hecs_shared::{Property, Replicate};

#[derive(Replicate)]
pub struct Position {
    pub x: Property<u8>,
    pub y: Property<u8>,
}

impl Position {
    pub fn new(x: u8, y: u8) -> Self {
        Self::new_complete(x, y)
    }
}
