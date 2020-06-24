
use std::{
    any::{TypeId},
    io::{Cursor},
};

use gaia_derive::Event;
use gaia_shared::{Event, EventBuilder, Property, PropertyIo};
use crate::ExampleEvent;

#[derive(Event, Clone)]
#[type_name = "ExampleEvent"]
pub struct StringEvent {
    pub message: Property<String>,
}

impl StringEvent {

    fn is_guaranteed() -> bool { true }

    pub fn new(message: String) -> StringEvent {
        return StringEvent::new_complete(message);
    }

    //TODO: Candidate for Macro
    pub fn new_complete(message: String) -> StringEvent {
        StringEvent {
            message: Property::<String>::new(message, 0),
        }
    }

    //TODO: Candidate for Macro
    fn read_to_type(buffer: &[u8]) -> ExampleEvent {
        let read_cursor = &mut Cursor::new(buffer);
        let mut message = Property::<String>::new(Default::default(), 0);
        message.read(read_cursor);

        return ExampleEvent::StringEvent(StringEvent {
            message,
        });
    }
}

//impl Event<ExampleEvent> for StringEvent {
//    //TODO: Candidate for Macro
//    fn write(&self, buffer: &mut Vec<u8>) {
//        PropertyIo::write(&self.message, buffer);
//    }
//}