
#[macro_use]
extern crate log;

#[macro_use]
extern crate slotmap;

pub use naia_shared::{find_my_ip_address, Config, EntityType, Entity};

mod naia_server;
mod client_connection;
mod server_event;
mod entities;
mod user;
mod room;
mod error;

pub use {
    naia_server::NaiaServer,
    server_event::ServerEvent,
    naia_server_socket::Packet,
    entities::{
        server_entity_manager::ServerEntityManager,
        server_entity_message::ServerEntityMessage,
        entity_packet_writer::EntityPacketWriter,
        entity_key::EntityKey,
        server_entity_mutator::ServerEntityMutator,
    },
    user::{UserKey},
};