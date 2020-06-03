
use gaia_shared::{ManifestType, NetEvent, NetBase};

use crate::StringEvent;

pub enum ExampleType {
    StringEvent(StringEvent)
}

impl ManifestType for ExampleType {
    fn optional_clone(&self) -> Option<Self> where Self : Sized {
        match self {
            ExampleType::StringEvent(identity) => {
                return Some(ExampleType::StringEvent(identity.clone()));
            }
        }
    }

    fn is_event(&self) -> bool {
        match self {
            ExampleType::StringEvent(identity) => {
                identity.is_event()
            }
        }
    }

    fn use_bytes(&mut self, bytes: &[u8]) {
        match self {
            ExampleType::StringEvent(identity) => {
                identity.read(bytes);
            }
        }
    }
}