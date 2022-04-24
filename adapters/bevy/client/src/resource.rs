use std::default::Default;

use naia_bevy_shared::Flag;

#[derive(Default)]
pub struct ClientResource {
    pub ticker: Flag,
    pub connector: Flag,
    pub disconnector: Flag,
}
