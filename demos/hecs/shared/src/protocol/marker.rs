use naia_shared::Replicate;

#[derive(Replicate)]
#[protocol_path = "crate::protocol::Protocol"]
pub struct Marker;

impl Default for Marker {
    fn default() -> Self {
        Marker::new_complete()
    }
}
