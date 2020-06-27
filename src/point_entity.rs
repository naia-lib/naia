
use gaia_derive::Entity;
use gaia_shared::{Entity, Property};

use crate::{ExampleEntity};

#[derive(Entity)]
#[type_name = "ExampleEntity"]
pub struct PointEntity {
    pub x: Property<u8>,
    pub y: Property<u8>,
}

impl PointEntity {

    pub fn new(x: u8, y: u8) -> PointEntity {
        return PointEntity::new_complete(x, y);
    }

    pub fn step(&mut self) {
        let mut x = *self.x.get();
        x += 1;
        if x > 20 {
            x = 0;
        }
        if x % 3 == 0 {
            let mut y = *self.y.get();
            y += 1;
            self.y.set(y);
        }
        self.x.set(x);
    }
}