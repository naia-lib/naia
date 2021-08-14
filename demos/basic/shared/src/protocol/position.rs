
use naia_derive::State;
use naia_shared::{State, Property};

use super::Protocol;

#[derive(State, Clone)]
#[type_name = "Protocol"]
pub struct Position {
    pub x: Property<u8>,
    pub y: Property<u8>,
}

impl Position {
    fn is_guaranteed() -> bool {
        false
    }

    pub fn new(x: u8, y: u8) -> Self {
        return Position::state_new_complete(
            x,
            y,
        );
    }
}
