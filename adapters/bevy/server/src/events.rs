use naia_server::{
    User, UserKey, shared::{Message, Channel}
};

pub struct ConnectEvent(pub UserKey);
pub struct DisconnectEvent(pub UserKey, pub User);
pub struct AuthEvents;
pub struct MessageEvents;
