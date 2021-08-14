
use naia_derive::State;
use naia_shared::{State, Property};

use super::Protocol;

#[derive(State, Clone)]
pub struct Position {
    pub x: Property<u8>,
    pub y: Property<u8>,
}

impl Position {
    pub fn new(x: u8, y: u8) -> Self {
        return Position::new_complete(
            x,
            y,
        );
    }
}
