use naia_shared::Replicate;

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Marker;

impl Marker {
    pub fn new() -> Self {
        Marker::new_complete()
    }
}
