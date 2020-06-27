
extern crate log;

pub use naia_shared::{find_my_ip_address, Config, EntityType};

mod naia_client;
mod server_connection;
mod client_event;
mod client_entity_message;
mod client_entity_manager;
mod client_connection_state;
mod error;

pub use {
    naia_client::NaiaClient,
    client_event::ClientEvent,
    naia_client_socket::Packet,
    client_entity_message::ClientEntityMessage,
    client_entity_manager::ClientEntityManager,
};