#[allow(clippy::module_inception)]
mod server;
pub use server::Server;

mod server_config;
pub use server_config::ServerConfig;

mod main_server;
pub use main_server::MainServer;
pub mod world_server;
pub use world_server::WorldServer;

mod scope_checks_cache;
mod scope_change;
mod room_store;
mod user_store;
