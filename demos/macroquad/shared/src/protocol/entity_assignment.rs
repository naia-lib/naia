use naia_derive::Replicate;
use naia_shared::Property;

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct EntityAssignment {
    pub assign: Property<bool>,
}

impl EntityAssignment {
    pub fn new(assign: bool) -> Self {
        return EntityAssignment::new_complete(assign);
    }
}
