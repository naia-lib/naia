use naia_derive::Replicate;
use naia_shared::{Property, Replicate};

use super::Protocol;

#[derive(Replicate, Clone)]
pub struct StringMessage {
    pub message: Property<String>,
}

impl StringMessage {
    pub fn new(message: String) -> StringMessage {
        return StringMessage::new_complete(message);
    }
}
