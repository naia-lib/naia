
use naia_shared::{Property, Replicate, Serde};

/// Here's an example of a Custom Property
#[derive(Serde, PartialEq, Clone)]
pub struct FullName {
    /// First name
    pub first: String,
    /// Last name
    pub last: String,
}

#[derive(Replicate)]
pub struct Character<T: 'static + Send + Sync + Serde> {
    pub something: Property<T>,
    pub x: Property<u8>,
    pub y: Property<u8>,
    pub fullname: Property<FullName>,
}

impl<T: 'static + Send + Sync + Serde> Character<T> {
    pub fn new(t: T, x: u8, y: u8, first: &str, last: &str) -> Self {
        Self::new_complete(
            t,
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
