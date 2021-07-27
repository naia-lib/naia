use naia_derive::Actor;
use naia_shared::{Actor, Property};

use super::Components;

#[derive(Actor)]
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
