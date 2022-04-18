use naia_shared::Replicate;

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Marker;

impl Marker {
    pub fn new() -> Self {
        return Marker::new_complete();
    }
}