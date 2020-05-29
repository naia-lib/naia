
#[macro_use]
extern crate log;

pub use gaia_shared::{find_my_ip_address, Config};

mod error;

mod gaia_server;
pub use gaia_server::GaiaServer;

mod server_event;
pub use server_event::ServerEvent;

pub use gaia_server_socket::Packet;