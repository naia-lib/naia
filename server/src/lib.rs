#[macro_use]
extern crate log;

#[macro_use]
extern crate slotmap;

pub use naia_shared::{find_my_ip_address, Config, Entity, EntityType};

mod client_connection;
mod entities;
mod error;
mod naia_server;
mod room;
mod server_event;
mod user;

pub use {
    entities::{
        entity_key::EntityKey, entity_packet_writer::EntityPacketWriter,
        server_entity_manager::ServerEntityManager, server_entity_message::ServerEntityMessage,
        server_entity_mutator::ServerEntityMutator,
    },
    naia_server::NaiaServer,
    naia_server_socket::Packet,
    server_event::ServerEvent,
    user::UserKey,
};
