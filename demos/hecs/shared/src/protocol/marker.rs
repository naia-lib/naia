use naia_hecs_shared::Replicate;

#[derive(Replicate)]
pub struct Marker;

impl Marker {
    pub fn new() -> Self {
        Self::new_complete()
    }
}
