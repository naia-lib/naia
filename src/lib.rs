
#[macro_use]
extern crate log;

pub use gaia_shared::{find_my_ip_address, Config, EntityType};

mod gaia_client;
mod client_connection;
mod client_event;
mod error;

pub use {
    gaia_client::GaiaClient,
    client_event::ClientEvent,
    client_connection::ClientConnection,
    gaia_client_socket::Packet
};