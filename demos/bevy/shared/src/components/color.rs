use bevy_ecs::prelude::Component;

use naia_bevy_shared::{Property, Replicate, Serde};

#[derive(Serde, PartialEq, Clone)]
pub enum ColorValue {
    Red,
    Blue,
    Yellow,
}

#[derive(Component, Replicate)]
pub struct Color {
    pub value: Property<ColorValue>,
}

impl Color {
    pub fn new(value: ColorValue) -> Self {
        Color::new_complete(value)
    }
}
