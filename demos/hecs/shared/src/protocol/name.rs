use naia_hecs_shared::{Property, Replicate, Serde};

// Here's an example of a Custom Property
#[derive(Serde, PartialEq, Clone)]
pub struct Fullname {
    pub first: String,
    pub last: String,
}

#[derive(Replicate)]
pub struct Name {
    pub full: Property<Fullname>,
}

impl Name {
    pub fn new(first: &str, last: &str) -> Self {
        Self::new_complete(Fullname {
            first: first.to_string(),
            last: last.to_string(),
        })
    }
}
