use naia_shared::{Property, Replicate};
#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Position {
    pub x: Property<u8>,
    pub y: Property<u8>,
}

impl Position {
    pub fn new(x: u8, y: u8) -> Self {
        Position::new_complete(x, y)
    }
}
