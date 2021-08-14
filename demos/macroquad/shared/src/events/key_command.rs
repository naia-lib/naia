use naia_derive::Event;
use naia_shared::{Event, Property};

use super::Events;

#[derive(Event, Clone)]
#[type_name = "Events"]
pub struct KeyCommand {
    pub w: Property<bool>,
    pub s: Property<bool>,
    pub a: Property<bool>,
    pub d: Property<bool>,
}

impl KeyCommand {
    pub fn new(w: bool, s: bool, a: bool, d: bool) -> KeyCommand {
        return KeyCommand::new_complete(w, s, a, d);
    }
}
