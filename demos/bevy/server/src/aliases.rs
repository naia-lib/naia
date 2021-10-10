use naia_bevy_server::{Entity, Event as NaiaServerEvent};
use naia_bevy_demo_shared::protocol::Protocol;

pub type ServerEvent = NaiaServerEvent<Protocol, Entity>;