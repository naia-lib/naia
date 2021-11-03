use naia_derive::{ProtocolType, Replicate};
use naia_shared::{Manifest, Property};

#[derive(Replicate, Clone)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Text {
    pub value: Property<String>,
}

impl Text {
    pub fn new(value: &str) -> Self {
        return Text::new_complete(value.to_string());
    }
}


#[derive(ProtocolType, Clone)]
pub enum Protocol {
    Text(Text),
}
