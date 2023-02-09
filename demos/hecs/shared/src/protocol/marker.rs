use naia_shared::Replicate;

#[derive(Replicate)]
pub struct Marker;

impl Marker {
    pub fn new() -> Self {
        Marker::new_complete()
    }
}
