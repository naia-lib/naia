
use naia_derive::State;
use naia_shared::{State, Property};

use super::Protocol;

#[derive(State, Clone)]
#[type_name = "Protocol"]
pub struct StringMessage {
    pub message: Property<String>,
}

impl StringMessage {
    fn is_guaranteed() -> bool {
        true
    }

    pub fn new(message: String) -> StringMessage {
        return StringMessage::state_new_complete(message);
    }
}
