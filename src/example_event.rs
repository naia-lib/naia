
use gaia_shared::{EventType, NetEvent};

use crate::{StringEvent};

pub enum ExampleEvent {
    StringEvent(StringEvent),
}

impl EventType for ExampleEvent {
    fn optional_clone(&self) -> Option<Self> where Self : Sized {
        match self {
            ExampleEvent::StringEvent(identity) => {
                return Some(ExampleEvent::StringEvent(identity.clone()));
            }
        }
    }

    fn use_bytes(&mut self, bytes: &[u8]) {
        match self {
            ExampleEvent::StringEvent(identity) => {
                identity.read(bytes);
            }
        }
    }
}