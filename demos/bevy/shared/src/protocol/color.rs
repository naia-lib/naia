use bevy_ecs::prelude::Component;

use naia_shared::{derive_serde, serde, Property, Replicate};

#[derive_serde]
pub enum ColorValue {
    Red,
    Blue,
    Yellow,
}

#[derive(Component, Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Color {
    pub value: Property<ColorValue>,
}

impl Color {
    pub fn new(value: ColorValue) -> Self {
        Color::new_complete(value)
    }
}
