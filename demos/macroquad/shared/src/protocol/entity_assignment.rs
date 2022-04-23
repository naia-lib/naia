use naia_shared::{EntityProperty, Property, Replicate};

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct EntityAssignment {
    pub entity: EntityProperty,
    pub assign: Property<bool>,
}

impl EntityAssignment {
    pub fn new(assign: bool) -> Self {
        EntityAssignment::new_complete(assign)
    }
}
