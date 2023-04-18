use naia_bevy_shared::{EntityRelation, Message};

#[derive(Message)]
pub struct KeyCommand {
    pub entity: EntityRelation,
    pub w: bool,
    pub s: bool,
    pub a: bool,
    pub d: bool,
}

impl KeyCommand {
    pub fn new(w: bool, s: bool, a: bool, d: bool) -> Self {
        Self {
            entity: EntityRelation::new(),
            w,
            s,
            a,
            d,
        }
    }
}
