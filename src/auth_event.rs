
use std::any::{TypeId};

use gaia_shared::{Event, EventBuilder};
use crate::ExampleEvent;

#[derive(Clone)]
pub struct AuthEvent {
    username: String,
    password: String,
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

    fn build(&self, buffer: &[u8]) -> ExampleEvent {
        let username_bytes_number: usize = (buffer[0] as usize) + 1;
        let username_bytes = &buffer[1..username_bytes_number];
        let password_bytes = &buffer[username_bytes_number..buffer.len()];
        let username = String::from_utf8_lossy(username_bytes).to_string();
        let password = String::from_utf8_lossy(password_bytes).to_string();
        return AuthEvent::new(&username, &password).to_type();
    }
}

impl AuthEvent {

    //TODO: Candidate for Macro
    pub fn get_builder() -> Box<dyn EventBuilder<ExampleEvent>> {
        return Box::new(AuthEventBuilder {
            type_id: TypeId::of::<AuthEvent>(),
        });
    }

    pub fn new(username: &str, password: &str) -> Self {
        AuthEvent {
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    pub fn get_username(&self) -> String {
        self.username.clone()
    }

    pub fn get_password(&self) -> String {
        self.password.clone()
    }
}

impl Event<ExampleEvent> for AuthEvent {
    fn is_guaranteed(&self) -> bool {
        false
    }

    fn write(&self, buffer: &mut Vec<u8>) {
        let mut bytes = self.username.as_bytes().to_vec();
        buffer.push(bytes.len() as u8);
        buffer.append(&mut bytes);
        bytes = self.password.as_bytes().to_vec();
        buffer.append(&mut bytes);
    }

    //TODO: Candidate for Macro
    fn to_type(&self) -> ExampleEvent {
        return ExampleEvent::AuthEvent(self.clone());
    }

    //TODO: Candidate for Macro
    fn get_type_id(&self) -> TypeId {
        return TypeId::of::<AuthEvent>();
    }
}