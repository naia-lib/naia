use naia_shared::{derive_serde, serde, Property, Replicate};

// Here's an example of a Custom Property
#[derive_serde]
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
        Character::new_complete(
            x,
            y,
            FullName {
                first: first.to_string(),
                last: last.to_string(),
            },
        )
    }

    pub fn step(&mut self) {
        *self.x += 1;
        if *self.x > 20 {
            *self.x = 0;
        }
        if *self.x % 3 == 0 {
            *self.y = self.y.wrapping_add(1);
        }
    }
}
