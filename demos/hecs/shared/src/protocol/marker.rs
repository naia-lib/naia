use naia_derive::ReplicateSafe;
use naia_shared::Property;

#[derive(ReplicateSafe, Clone)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Marker {
    pub name: Property<String>,
}

impl Marker {
    pub fn new(name: &str) -> Self {
        return Marker::new_complete(name.to_string());
    }
}
