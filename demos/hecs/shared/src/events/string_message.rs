
use naia_derive::Event;
use naia_shared::{Event, Property};

use super::Events;

#[derive(Event, Clone)]
#[type_name = "Events"]
pub struct StringMessage {
    pub message: Property<String>,
}

impl StringMessage {
    pub fn new(message: String) -> StringMessage {
        return StringMessage::new_complete(message);
    }
}
