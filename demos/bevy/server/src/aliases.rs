use naia_server::Server as NaiaServer;
use naia_bevy_server::Entity;
use naia_bevy_demo_shared::protocol::Protocol;

pub type Server = NaiaServer<Protocol, Entity>;