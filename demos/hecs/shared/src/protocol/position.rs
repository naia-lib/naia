use naia_derive::Replicate;
use naia_shared::{Property, Replicate};

use super::Protocol;

#[derive(Replicate, Clone)]
pub struct Position {
    pub x: Property<u8>,
    pub y: Property<u8>,
}

impl Position {
    pub fn new(x: u8, y: u8) -> Self {
        return Position::new_complete(x, y);
    }
}
