
use gaia_shared::{EventType, Event};

use crate::{StringEvent};

#[derive(Clone)]
pub enum ExampleEvent {
    StringEvent(StringEvent),
}

impl EventType for ExampleEvent {
    fn read(&mut self, bytes: &[u8]) {
        match self {
            ExampleEvent::StringEvent(identity) => {
                identity.read(bytes);
            }
        }
    }
}