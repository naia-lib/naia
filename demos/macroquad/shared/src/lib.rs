extern crate log;
extern crate naia_shared;

pub mod behavior;
pub mod protocol;

mod channels;
pub use channels::Channels;

mod shared;
pub use shared::shared_config;
