use naia_server::{
    shared::{ChannelIndex, Protocolize},
    User, UserKey,
};

pub struct AuthorizationEvent<P: Protocolize>(pub UserKey, pub P);
pub struct ConnectionEvent(pub UserKey);
pub struct DisconnectionEvent(pub UserKey, pub User);
pub struct MessageEvent<P: Protocolize, C: ChannelIndex>(pub UserKey, pub C, pub P);
