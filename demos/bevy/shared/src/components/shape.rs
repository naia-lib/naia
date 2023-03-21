use bevy_ecs::prelude::Component;

use naia_bevy_shared::{Property, Replicate, Serde};

#[derive(Serde, PartialEq, Clone)]
pub enum ShapeValue {
    Square,
    Circle,
}

#[derive(Component, Replicate)]
pub struct Shape {
    pub value: Property<ShapeValue>,
}

impl Shape {
    pub fn new(value: ShapeValue) -> Self {
        Self::new_complete(value)
    }
}
