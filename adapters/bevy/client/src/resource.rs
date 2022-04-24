use std::default::Default;

use naia_bevy_shared::Flag;

pub struct ClientResource {
    pub ticker: Flag,
    pub connector: Flag,
    pub disconnector: Flag,
}

impl Default for ClientResource {
    fn default() -> Self {
        Self {
            ticker: Flag::new(),
            connector: Flag::new(),
            disconnector: Flag::new(),
        }
    }
}
