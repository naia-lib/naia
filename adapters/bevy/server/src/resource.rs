use std::default::Default;

use naia_bevy_shared::Flag;

#[derive(Default)]
pub struct ServerResource {
    pub ticker: Flag,
}
