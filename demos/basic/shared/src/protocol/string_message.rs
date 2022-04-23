use naia_shared::{Property, Replicate};

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct StringMessage {
    pub contents: Property<String>,
}

impl StringMessage {
    pub fn new(contents: String) -> Self {
        StringMessage::new_complete(contents)
    }
}
