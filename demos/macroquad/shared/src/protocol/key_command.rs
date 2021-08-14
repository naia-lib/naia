
use naia_derive::Replicate;
use naia_shared::{Replicate, Property};

use super::Protocol;

#[derive(Replicate, Clone)]
pub struct KeyCommand {
    pub w: Property<bool>,
    pub s: Property<bool>,
    pub a: Property<bool>,
    pub d: Property<bool>,
}

impl KeyCommand {
    pub fn new(w: bool, s: bool, a: bool, d: bool) -> KeyCommand {
        return KeyCommand::new_complete(w, s, a, d);
    }
}
