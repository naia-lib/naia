use nanoserde::{DeBin, SerBin};

use naia_derive::Replicate;
use naia_shared::Property;

// Here's an example of a Custom Property
#[derive(Default, PartialEq, Clone, DeBin, SerBin)]
pub struct Fullname {
    pub first: String,
    pub last: String,
}

#[derive(Replicate, Clone)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Name {
    pub full: Property<Fullname>,
}

impl Name {
    pub fn new(first: &str, last: &str) -> Self {
        return Name::new_complete(Fullname {
            first: first.to_string(),
            last: last.to_string(),
        });
    }
}
