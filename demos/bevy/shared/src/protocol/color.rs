use bevy_ecs::prelude::Component;
use nanoserde::{DeBin, SerBin};

use naia_derive::Replicate;
use naia_shared::Property;

#[derive(Clone, PartialEq, DeBin, SerBin)]
pub enum ColorValue {
    Red,
    Blue,
    Yellow,
}

impl Default for ColorValue {
    fn default() -> Self {
        ColorValue::Red
    }
}

#[derive(Component, Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Color {
    pub value: Property<ColorValue>,
}

impl Color {
    pub fn new(value: ColorValue) -> Self {
        return Color::new_complete(value);
    }
}
