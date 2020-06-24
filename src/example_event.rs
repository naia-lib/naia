
use gaia_derive::EventType;

use crate::{StringEvent, AuthEvent};

#[derive(EventType, Clone)]
pub enum ExampleEvent {
    StringEvent(StringEvent),
    AuthEvent(AuthEvent),
}