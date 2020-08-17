use nanoserde::{DeBin, SerBin};

use naia_derive::Entity;
use naia_shared::{Entity, Property};

use crate::ExampleEntity;

// Here's an example of a Custom Property
#[derive(Default, Clone, DeBin, SerBin)]
pub struct Name {
    pub first: String,
    pub last: String,
}

#[derive(Entity)]
#[type_name = "ExampleEntity"]
pub struct PointEntity {
    pub x: Property<u8>,
    pub y: Property<u8>,
    pub name: Property<Name>,
}

impl PointEntity {
    pub fn new(x: u8, y: u8, first: &str, last: &str) -> PointEntity {
        return PointEntity::new_complete(
            x,
            y,
            Name {
                first: first.to_string(),
                last: last.to_string(),
            },
        );
    }

    pub fn step(&mut self) {
        let mut x = *self.x.get();
        x += 1;
        if x > 20 {
            x = 0;
        }
        if x % 3 == 0 {
            let mut y = *self.y.get();
            y = y.wrapping_add(1);
            self.y.set(y);
        }
        self.x.set(x);
    }
}
