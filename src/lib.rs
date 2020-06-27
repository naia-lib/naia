
extern crate log;

pub use gaia_shared::{find_my_ip_address, Config, EntityType};

mod gaia_client;
mod server_connection;
mod client_event;
mod client_entity_message;
mod client_entity_manager;
mod client_connection_state;
mod error;

pub use {
    gaia_client::GaiaClient,
    client_event::ClientEvent,
    gaia_client_socket::Packet,
    client_entity_message::ClientEntityMessage,
    client_entity_manager::ClientEntityManager,
};