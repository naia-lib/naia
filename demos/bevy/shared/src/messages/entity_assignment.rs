use naia_bevy_shared::{EntityRelation, Message};

#[derive(Message)]
pub struct EntityAssignment {
    pub entity: EntityRelation,
    pub assign: bool,
}

impl EntityAssignment {
    pub fn new(assign: bool) -> Self {
        Self {
            assign,
            entity: EntityRelation::new(),
        }
    }
}
