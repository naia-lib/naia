use naia_derive::ReplicateSafe;
use naia_shared::Property;

use super::Protocol;

#[derive(ReplicateSafe, Clone)]
pub struct KeyCommand {
    pub w: Property<bool>,
    pub s: Property<bool>,
    pub a: Property<bool>,
    pub d: Property<bool>,
}

impl KeyCommand {
    pub fn new(w: bool, s: bool, a: bool, d: bool) -> Self {
        return KeyCommand::new_complete(w, s, a, d);
    }
}
