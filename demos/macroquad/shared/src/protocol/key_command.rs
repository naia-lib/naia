use naia_shared::{EntityProperty, Property, Replicate};

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct KeyCommand {
    pub entity: EntityProperty,
    pub w: Property<bool>,
    pub s: Property<bool>,
    pub a: Property<bool>,
    pub d: Property<bool>,
}

impl KeyCommand {
    pub fn new(w: bool, s: bool, a: bool, d: bool) -> Self {
        return KeyCommand::new_complete(w, s, a, d);
    }
}
