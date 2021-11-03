use naia_bevy_shared::Flag;

pub struct ServerResource {
    pub ticker: Flag,
}

impl ServerResource {
    pub fn new() -> Self {
        Self {
            ticker: Flag::new(),
        }
    }
}
