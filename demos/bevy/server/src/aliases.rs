use naia_bevy_demo_shared::protocol::Protocol;
use naia_bevy_server::{Entity, Event as NaiaServerEvent};

pub type ServerEvent = NaiaServerEvent<Protocol, Entity>;
