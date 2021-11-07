use nanoserde::{DeBin, SerBin};

use naia_derive::Replicate;
use naia_shared::Property;

// Here's an example of a Custom Property
#[derive(Default, PartialEq, Clone, DeBin, SerBin)]
pub struct FullName {
    pub first: String,
    pub last: String,
}

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Character {
    pub x: Property<u8>,
    pub y: Property<u8>,
    pub fullname: Property<FullName>,
}

impl Character {
    pub fn new(x: u8, y: u8, first: &str, last: &str) -> Self {
        return Character::new_complete(
            x,
            y,
            FullName {
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
