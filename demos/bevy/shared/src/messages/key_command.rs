use naia_bevy_shared::{EntityProperty, Message};

#[derive(Message)]
pub struct KeyCommand {
    pub entity: EntityProperty,
    pub w: bool,
    pub s: bool,
    pub a: bool,
    pub d: bool,
}

impl KeyCommand {
    pub fn new(w: bool, s: bool, a: bool, d: bool) -> Self {
        Self {
            entity: EntityProperty::new_for_message(),
            w,
            s,
            a,
            d,
        }
    }
}
