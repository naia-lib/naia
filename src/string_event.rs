
use std::{
    any::{TypeId},
    io::{Cursor},
};

use gaia_shared::{Event, EventBuilder, Property, PropertyIo};
use crate::ExampleEvent;

#[derive(Clone)]
pub struct StringEvent {
    pub message: Property<String>,//TODO: Candidate for Macro
}

//TODO: Candidate for Macro
pub struct StringEventBuilder {
    type_id: TypeId,
}

impl EventBuilder<ExampleEvent> for StringEventBuilder {
    //TODO: Candidate for Macro
    fn get_type_id(&self) -> TypeId {
        return self.type_id;
    }

    //TODO: Candidate for Macro
    fn build(&self, buffer: &[u8]) -> ExampleEvent {
        return StringEvent::read_to_type(buffer);
    }
}

impl StringEvent {

    fn is_guaranteed() -> bool { true }

    pub fn new(message: String) -> StringEvent {
        return StringEvent::new_complete(message);
    }

    //TODO: Candidate for Macro
    pub fn get_builder() -> Box<dyn EventBuilder<ExampleEvent>> {
        return Box::new(StringEventBuilder {
            type_id: TypeId::of::<StringEvent>(),
        });
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

impl Event<ExampleEvent> for StringEvent {
    fn is_guaranteed(&self) -> bool {
        StringEvent::is_guaranteed()
    }

    //TODO: Candidate for Macro
    fn write(&self, buffer: &mut Vec<u8>) {
        PropertyIo::write(&self.message, buffer);
    }

    //TODO: Candidate for Macro
    fn get_typed_copy(&self) -> ExampleEvent {
        return ExampleEvent::StringEvent(self.clone());
    }

    //TODO: Candidate for Macro
    fn get_type_id(&self) -> TypeId {
        return TypeId::of::<StringEvent>();
    }
}