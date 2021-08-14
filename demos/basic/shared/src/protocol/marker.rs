use naia_derive::Replicate;
use naia_shared::{Replicate, Property};

use super::Protocol;

#[derive(Replicate, Clone)]
pub struct Marker {
    pub name: Property<String>,
}

impl Marker {
    pub fn new(name: &str) -> Self {
        return Marker::new_complete(
            name.to_string()
        );
    }
}
