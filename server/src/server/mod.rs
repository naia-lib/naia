#[allow(clippy::module_inception)]
mod server;
pub use server::Server;

mod server_config;
pub use server_config::ServerConfig;

mod main_server;
pub use main_server::MainServer;
pub mod world_server;
pub use world_server::InternalWorldServer;
pub(crate) use world_server::user_scope_has_entity_impl;

/// The unified `WorldServer` enum (G-unify Phase 2c) over the resident +
/// pipelined engine shapes.
pub mod world_server_enum;
pub use world_server_enum::{ServerMode, WorldServer};

pub mod connection_shared;
pub use connection_shared::ConnectionShared;

pub mod server_shared;
pub use server_shared::ServerShared;

pub mod receive_output;
pub use receive_output::ReceiveOutput;

pub mod pipeline_handles;
pub use pipeline_handles::{RecvHandle, SendHandle};

pub mod recv_state;
pub use recv_state::RecvState;

pub mod send_state;
pub use send_state::SendState;

pub mod coord_state;
pub use coord_state::CoordinatorState;

pub mod send_state_update;
pub use send_state_update::SendStateUpdate;

pub(crate) mod configure_replication;
pub(crate) mod room_store;
pub(crate) mod scope_change;
pub(crate) mod scope_checks_cache;
pub(crate) mod user_store;
