
#[macro_use]
extern crate log;

pub use gaia_shared::{find_my_ip_address, Config, EntityType};

mod gaia_client;
mod server_connection;
mod client_event;
mod error;

pub use {
    gaia_client::GaiaClient,
    client_event::ClientEvent,
    server_connection::ServerConnection,
    gaia_client_socket::Packet
};