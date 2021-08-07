use naia_derive::State;
use naia_shared::{State, Property};

use super::Components;

#[derive(State)]
#[type_name = "Components"]
pub struct Marker {
    pub name: Property<String>,
}

impl Marker {
    pub fn new(name: &str) -> Self {
        return Marker::state_new_complete(
            name.to_string()
        );
    }
}
