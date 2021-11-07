use naia_derive::Replicate;
use naia_shared::Property;

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Text {
    pub value: Property<String>,
}

impl Text {
    pub fn new(value: &str) -> Self {
        return Text::new_complete(value.to_string());
    }
}
