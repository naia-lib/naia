use naia_shared::{derive_serde, serde, Property, Replicate};

// Here's an example of a Custom Property
#[derive_serde]
pub struct Fullname {
    pub first: String,
    pub last: String,
}

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Name {
    pub full: Property<Fullname>,
}

impl Name {
    pub fn new(first: &str, last: &str) -> Self {
        Name::new_complete(Fullname {
            first: first.to_string(),
            last: last.to_string(),
        })
    }
}
