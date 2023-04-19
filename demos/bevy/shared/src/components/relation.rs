use bevy_ecs::prelude::Component;

use naia_bevy_shared::{EntityProperty, Replicate};

#[derive(Component, Replicate)]
pub struct Relation {
    pub entity: EntityProperty,
}

impl Relation {
    pub fn new() -> Self {
        Self::new_complete()
    }
}
