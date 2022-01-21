extern crate log;
extern crate naia_derive;

pub mod behavior;
pub mod protocol;

mod shared;
pub use shared::{get_server_address, get_shared_config};
