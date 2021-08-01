extern crate log;
extern crate naia_derive;

use std::net::SocketAddr;

pub mod events;
pub mod behavior;
pub mod objects;
mod manifest_load;
mod shared_config;

pub use manifest_load::manifest_load;
pub use shared_config::get_shared_config;

pub fn get_server_address() -> SocketAddr {
    return "127.0.0.1:14191"
        .parse()
        .expect("could not parse socket address from string");
}