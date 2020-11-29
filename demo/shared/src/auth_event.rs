use crate::ExampleEvent;
use naia_derive::Event;
use naia_shared::{Event, Property};

#[derive(Event, Clone)]
#[type_name = "ExampleEvent"]
pub struct AuthEvent {
    pub username: Property<String>,
    pub password: Property<String>,
}

impl AuthEvent {
    fn is_guaranteed() -> bool {
        false
    }

    pub fn new(username: &str, password: &str) -> AuthEvent {
        return AuthEvent::new_complete(username.to_string(), password.to_string());
    }
}
