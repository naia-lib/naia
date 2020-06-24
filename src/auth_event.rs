
use std::{
    any::{TypeId},
    io::{Cursor},
};

use gaia_shared::{Event, EventBuilder, Property, PropertyIo};
use crate::ExampleEvent;

#[derive(Clone)]
pub struct AuthEvent {
    pub username: Property<String>,
    pub password: Property<String>,
}

//TODO: Candidate for Macro
pub struct AuthEventBuilder {
    type_id: TypeId,
}

impl EventBuilder<ExampleEvent> for AuthEventBuilder {

    //TODO: Candidate for Macro
    fn get_type_id(&self) -> TypeId {
        return self.type_id;
    }

    //TODO: Candidate for Macro
    fn build(&self, buffer: &[u8]) -> ExampleEvent {
        return AuthEvent::read_to_type(buffer);
    }
}

impl AuthEvent {

    pub fn new(username: &str, password: &str) -> AuthEvent {
        return AuthEvent::new_complete(username.to_string(), password.to_string());
    }

    //TODO: Candidate for Macro
    pub fn get_builder() -> Box<dyn EventBuilder<ExampleEvent>> {
        return Box::new(AuthEventBuilder {
            type_id: TypeId::of::<AuthEvent>(),
        });
    }

    //TODO: Candidate for Macro
    pub fn new_complete(username: String, password: String) -> Self {
        AuthEvent {
            username: Property::<String>::new(username, 0),
            password: Property::<String>::new(password, 0),
        }
    }

    //TODO: Candidate for Macro
    fn read_to_type(buffer: &[u8]) -> ExampleEvent {
        let read_cursor = &mut Cursor::new(buffer);
        let mut username = Property::<String>::new(Default::default(), 0);
        username.read(read_cursor);
        let mut password = Property::<String>::new(Default::default(), 0);
        password.read(read_cursor);

        return ExampleEvent::AuthEvent(AuthEvent {
            username,
            password
        });
    }
}

impl Event<ExampleEvent> for AuthEvent {
    fn is_guaranteed(&self) -> bool {
        false
    }

    //TODO: Candidate for Macro
    fn write(&self, buffer: &mut Vec<u8>) {
        PropertyIo::write(&self.username, buffer);
        PropertyIo::write(&self.password, buffer);
    }

    //TODO: Candidate for Macro
    fn get_typed_copy(&self) -> ExampleEvent {
        return ExampleEvent::AuthEvent(self.clone());
    }

    //TODO: Candidate for Macro
    fn get_type_id(&self) -> TypeId {
        return TypeId::of::<AuthEvent>();
    }
}