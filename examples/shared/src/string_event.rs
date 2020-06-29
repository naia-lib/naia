use crate::ExampleEvent;
use naia_derive::Event;
use naia_shared::{Event, Property};

#[derive(Event, Clone)]
#[type_name = "ExampleEvent"]
pub struct StringEvent {
    pub message: Property<String>,
}

impl StringEvent {
    fn is_guaranteed() -> bool {
        true
    }

    pub fn new(message: String) -> StringEvent {
        return StringEvent::new_complete(message);
    }
}
