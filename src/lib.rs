
#[macro_use]
extern crate log;

#[macro_use]
extern crate cfg_if;

mod ack_manager;
mod config;
mod event_manager;
mod server_entity_manager;
mod client_entity_manager;
mod net_connection;
mod packet_type;
mod sequence_buffer;
mod standard_header;
mod timestamp;
mod event_manifest;
mod entity_manifest;
mod net_event;
mod net_entity;
mod manager_type;
mod packet_reader;
mod packet_writer;
mod event_type;
mod entity_type;
mod entity_store;
mod state_mask;
mod host_type;
mod entity_manager;

pub mod utils;

pub use gaia_socket_shared::{find_my_ip_address, Timer};

pub use packet_type::PacketType;
pub use standard_header::StandardHeader;
pub use config::Config;
pub use net_connection::NetConnection;
pub use timestamp::Timestamp;
pub use net_event::{NetEvent, NetEventType, NetEventClone};
pub use net_entity::{NetEntity, NetEntityType};
pub use event_manifest::EventManifest;
pub use entity_manifest::EntityManifest;
pub use entity_manager::EntityManager;
pub use manager_type::ManagerType;
pub use packet_reader::PacketReader;
pub use packet_writer::PacketWriter;
pub use event_type::EventType;
pub use entity_type::EntityType;
pub use entity_store::{EntityKey, EntityStore};
pub use state_mask::StateMask;
pub use host_type::HostType;