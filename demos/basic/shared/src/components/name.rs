use nanoserde::{DeBin, SerBin};

use naia_derive::State;
use naia_shared::{State, Property};

use super::Components;

// Here's an example of a Custom Property
#[derive(Default, PartialEq, Clone, DeBin, SerBin)]
pub struct Fullname {
    pub first: String,
    pub last: String,
}

#[derive(State)]
#[type_name = "Components"]
pub struct Name {
    pub full: Property<Fullname>,
}

impl Name {
    pub fn new(first: &str, last: &str) -> Self {
        return Name::state_new_complete(
            Fullname {
                first:  first.to_string(),
                last:   last.to_string(),
            }
        );
    }
}
