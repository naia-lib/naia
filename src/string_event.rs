
use std::{
    any::{TypeId},
    io::{Cursor},
};

use gaia_derive::Event;
use gaia_shared::{Event, Property};
use crate::ExampleEvent;

#[derive(Event, Clone)]
#[type_name = "ExampleEvent"]
pub struct StringEvent {
    pub message: Property<String>,
}

impl StringEvent {

    fn is_guaranteed() -> bool { true }

    pub fn new(message: String) -> StringEvent {
        return StringEvent::new_complete(message);
    }
}