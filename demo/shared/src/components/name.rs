
use naia_derive::Actor;
use naia_shared::{Actor, Property};

use super::Components;

#[derive(Actor)]
#[type_name = "Components"]
pub struct Name {
    pub first: Property<String>,
    pub last: Property<String>,
}

impl Name {
    pub fn new(first: &str, last: &str) -> Self {
        return Name::new_complete(
            first.to_string(),
            last.to_string(),
        );
    }
}
