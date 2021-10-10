use naia_bevy_demo_shared::protocol::Protocol;
use naia_bevy_server::{Entity, Event as NaiaServerEvent, Server as NaiaServer};

pub type Server<'s> = NaiaServer<'s, Protocol>;
pub type ServerEvent = NaiaServerEvent<Protocol, Entity>;
