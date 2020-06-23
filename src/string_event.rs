
use std::any::{TypeId};

use gaia_shared::{Event, EventBuilder};
use crate::ExampleEvent;

#[derive(Clone)]
pub struct StringEvent {
    msg: String,
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

    fn build(&self, buffer: &[u8]) -> ExampleEvent {
        let msg = String::from_utf8_lossy(buffer).to_string();
        return StringEvent::new(msg).to_type();
    }
}

impl StringEvent {
    //TODO: Candidate for Macro
    pub fn get_builder() -> Box<dyn EventBuilder<ExampleEvent>> {
        return Box::new(StringEventBuilder {
            type_id: TypeId::of::<StringEvent>(),
        });
    }

    pub fn new(msg: String) -> Self {
        StringEvent {
            msg,
        }
    }

    pub fn get_message(&self) -> String {
        self.msg.clone()
    }
}

impl Event<ExampleEvent> for StringEvent {
    fn is_guaranteed(&self) -> bool {
        true
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        let mut bytes = self.msg.as_bytes().to_vec();
        buffer.append(&mut bytes);
    }

    //TODO: Candidate for Macro
    fn to_type(&self) -> ExampleEvent {
        return ExampleEvent::StringEvent(self.clone());
    }

    //TODO: Candidate for Macro
    fn get_type_id(&self) -> TypeId {
        return TypeId::of::<StringEvent>();
    }
}