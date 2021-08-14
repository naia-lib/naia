use naia_derive::Replicate;
use naia_shared::{Replicate, Property};

use super::Components;

#[derive(Replicate)]
#[type_name = "Components"]
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
