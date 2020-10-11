use nanoserde::{DeBin, SerBin};

use naia_derive::Actor;
use naia_shared::{Actor, Property};

use crate::ExampleActor;

// Here's an example of a Custom Property
#[derive(Default, Clone, DeBin, SerBin)]
pub struct Name {
    pub first: String,
    pub last: String,
}

#[derive(Actor)]
#[type_name = "ExampleActor"]
pub struct PointActor {
    pub x: Property<u8>,
    pub y: Property<u8>,
    pub name: Property<Name>,
}

impl PointActor {
    pub fn new(x: u8, y: u8, first: &str, last: &str) -> PointActor {
        return PointActor::new_complete(
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
