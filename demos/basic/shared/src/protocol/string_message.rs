
use naia_derive::State;
use naia_shared::{State, Property};

use super::Protocol;

#[derive(State, Clone)]
pub struct StringMessage {
    pub message: Property<String>,
}

impl StringMessage {
    fn is_guaranteed() -> bool {
        true
    }

    pub fn new(message: String) -> StringMessage {
        return StringMessage::new_complete(message);
    }
}
