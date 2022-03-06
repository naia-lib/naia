use naia_shared::{EntityHandle, EntityProperty, Property, Replicate};

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct EntityAssignment {
    pub entity: EntityProperty,
    pub assign: Property<bool>,
}

impl EntityAssignment {
    pub fn new(entity_handle: EntityHandle, assign: bool) -> Self {
        return EntityAssignment::new_complete(entity_handle, assign);
    }
}
