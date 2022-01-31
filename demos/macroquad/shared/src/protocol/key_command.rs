use naia_derive::Replicate;
use naia_shared::{EntityNetId, Property};

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct KeyCommand {
    pub entity_net_id: Property<EntityNetId>,
    pub w: Property<bool>,
    pub s: Property<bool>,
    pub a: Property<bool>,
    pub d: Property<bool>,
}

impl KeyCommand {
    pub fn new(entity_net_id: EntityNetId, w: bool, s: bool, a: bool, d: bool) -> Self {
        return KeyCommand::new_complete(entity_net_id, w, s, a, d);
    }
}
