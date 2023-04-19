use bevy_ecs::prelude::Component;

use naia_bevy_shared::{EntityProperty, Property, Replicate, Serde};

#[derive(Component, Replicate)]
pub struct Relation {
    pub entity: EntityProperty,
}

impl Relation {
    pub fn new() -> Self {
        Self::new_complete()
    }
}
