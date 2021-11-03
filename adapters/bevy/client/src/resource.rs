use naia_bevy_shared::Flag;

pub struct ClientResource {
    pub ticker: Flag,
    pub connector: Flag,
    pub disconnector: Flag,
}

impl ClientResource {
    pub fn new() -> Self {
        Self {
            ticker: Flag::new(),
            connector: Flag::new(),
            disconnector: Flag::new(),
        }
    }
}
