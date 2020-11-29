use naia_derive::EventType;

use crate::{AuthEvent, StringEvent};

#[derive(EventType, Clone)]
pub enum ExampleEvent {
    StringEvent(StringEvent),
    AuthEvent(AuthEvent),
}
