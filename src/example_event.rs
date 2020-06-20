
use std::{
    any::{TypeId},
};

use gaia_shared::{EventType, Event, EventTypeGetter};

use crate::{StringEvent, AuthEvent};

#[derive(Clone)]
pub enum ExampleEvent {
    StringEvent(StringEvent),
    AuthEvent(AuthEvent),
}

impl EventType for ExampleEvent {
    fn read(&mut self, bytes: &[u8]) {
        match self {
            ExampleEvent::StringEvent(identity) => {
                identity.read(bytes);
            }
            ExampleEvent::AuthEvent(identity) => {
                identity.read(bytes);
            }
        }
    }

    fn write(&mut self, buffer: &mut Vec<u8>) {
        match self {
            ExampleEvent::StringEvent(identity) => {
                identity.write(buffer);
            }
            ExampleEvent::AuthEvent(identity) => {
                identity.write(buffer);
            }
        }
    }

    fn get_type_id(&self) -> TypeId {
        match self {
            ExampleEvent::StringEvent(identity) => {
                return identity.get_type_id();
            }
            ExampleEvent::AuthEvent(identity) => {
                return identity.get_type_id();
            }
        }
    }
}