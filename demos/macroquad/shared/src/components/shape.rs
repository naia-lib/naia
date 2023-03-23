use naia_shared::{Property, Replicate, Serde};

#[derive(Serde, PartialEq, Clone)]
pub enum ShapeValue {
    Square,
    Circle,
}

#[derive(Replicate)]
pub struct Shape {
    pub value: Property<ShapeValue>,
}

impl Shape {
    pub fn new(value: ShapeValue) -> Self {
        Self::new_complete(value)
    }
}
