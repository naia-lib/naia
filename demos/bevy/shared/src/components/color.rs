use bevy_ecs::prelude::Component;

use naia_bevy_shared::{Property, Replicate, Serde};

#[derive(Serde, PartialEq, Clone)]
pub enum ColorValue {
    Red,
    Blue,
    Yellow,
    Green,
    White,
    Purple,
    Orange,
    Aqua,
}

#[derive(Component, Replicate)]
pub struct Color {
    pub value: Property<ColorValue>,
}

impl Color {
    pub fn new(value: ColorValue) -> Self {
        Self::new_complete(value)
    }
}
