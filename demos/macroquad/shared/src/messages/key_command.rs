use naia_shared::{EntityProperty, Message};

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
            w,
            s,
            a,
            d,
            entity: EntityProperty::new(),
        }
    }
}
