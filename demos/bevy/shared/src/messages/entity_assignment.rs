use naia_bevy_shared::{EntityProperty, Message};

#[derive(Message)]
pub struct EntityAssignment {
    pub entity: EntityProperty,
    pub assign: bool,
}

impl EntityAssignment {
    pub fn new(assign: bool) -> Self {
        Self {
            assign,
            entity: EntityProperty::new(),
        }
    }
}
