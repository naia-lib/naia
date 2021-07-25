
use naia_derive::Actor;
use naia_shared::{Actor, Property};

use super::Components;

#[derive(Actor)]
#[type_name = "Components"]
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
