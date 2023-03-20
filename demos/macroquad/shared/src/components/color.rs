use naia_shared::{Property, Replicate, Serde};

#[derive(Serde, PartialEq, Clone)]
pub enum ColorValue {
    Red,
    Blue,
    Yellow,
    Green,
}

#[derive(Replicate)]
pub struct Color {
    pub value: Property<ColorValue>,
}

impl Color {
    pub fn new(value: ColorValue) -> Self {
        Color::new_complete(value)
    }
}
