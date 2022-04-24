use std::default::Default;

use naia_bevy_shared::Flag;

pub struct ServerResource {
    pub ticker: Flag,
}

impl Default for ServerResource {
    fn default() -> Self {
        Self {
            ticker: Flag::default(),
        }
    }
}
