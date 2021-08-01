use naia_derive::EventType;

use crate::{AuthEvent, KeyCommand};

#[derive(EventType, Clone)]
pub enum ExampleEvent {
    KeyCommand(KeyCommand),
    AuthEvent(AuthEvent),
}
