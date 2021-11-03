#![feature(const_type_id)]

extern crate log;
extern crate naia_derive;

mod shared;
pub use shared::{get_server_address, get_shared_config};
