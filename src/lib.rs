
#[macro_use]
extern crate log;

#[macro_use]
extern crate slotmap;

pub use gaia_shared::{find_my_ip_address, Config, EntityType, Entity};

mod gaia_server;
mod client_connection;
mod server_event;
mod entities;
mod user;
mod room;
mod error;

pub use {
    gaia_server::GaiaServer,
    server_event::ServerEvent,
    gaia_server_socket::Packet,
    entities::{
        server_entity_manager::ServerEntityManager,
        server_entity_message::ServerEntityMessage,
        entity_packet_writer::EntityPacketWriter,
        entity_key::EntityKey,
        server_entity_mutator::ServerEntityMutator,
    },
    user::{UserKey},
};