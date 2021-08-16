use naia_derive::Replicate;
use naia_shared::Property;

use super::Protocol;

#[derive(Replicate, Clone)]
pub struct StringMessage {
    pub contents: Property<String>,
}

impl StringMessage {
    pub fn new(contents: String) -> Ref<Self> {
        return StringMessage::new_complete(contents);
    }
}
