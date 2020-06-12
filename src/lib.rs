
#[macro_use]
extern crate log;

pub use gaia_shared::{find_my_ip_address, Config};

mod gaia_server;
mod client_connection;
mod server_event;
mod server_entity_manager;
mod server_entity_message;
mod entity_packet_writer;
mod error;

pub use {
    gaia_server::GaiaServer,
    server_event::ServerEvent,
    gaia_server_socket::Packet,
    server_entity_manager::ServerEntityManager,
    server_entity_message::ServerEntityMessage,
    entity_packet_writer::EntityPacketWriter,
};