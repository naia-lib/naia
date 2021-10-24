use naia_derive::Replicate;
use naia_shared::Property;

use super::Protocol;

#[derive(Replicate, Clone)]
pub struct Position {
    pub x: Property<i16>,
    pub y: Property<i16>,
}

impl Position {
    pub fn new(x: i16, y: i16) -> Self {
        return Position::new_complete(x, y);
    }
}
