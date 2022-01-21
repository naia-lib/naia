use bevy_ecs::prelude::Component;

use naia_derive::Replicate;
use naia_shared::Property;

#[derive(Replicate, Component)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct KeyCommand {
    pub w: Property<bool>,
    pub s: Property<bool>,
    pub a: Property<bool>,
    pub d: Property<bool>,
}

impl KeyCommand {
    pub fn new(w: bool, s: bool, a: bool, d: bool) -> Self {
        return KeyCommand::new_complete(w, s, a, d);
    }
}
