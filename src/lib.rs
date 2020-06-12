
#[macro_use]
extern crate log;

pub use gaia_shared::{find_my_ip_address, Config, EntityType};

mod error;

mod gaia_client;
pub use gaia_client::GaiaClient;

mod client_event;
pub use client_event::ClientEvent;

pub use gaia_client_socket::Packet;