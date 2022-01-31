use naia_derive::Replicate;
use naia_shared::{Property, EntityNetId};

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct EntityAssignment {
    pub assign: Property<bool>,
    pub entity_net_id: Property<EntityNetId>,
}

impl EntityAssignment {
    pub fn new(assign: bool, entity_net_id: EntityNetId) -> Self {
        return EntityAssignment::new_complete(assign, entity_net_id);
    }
}
