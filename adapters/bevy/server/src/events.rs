use bevy::ecs::entity::Entity;

use naia_server::{shared::Protocolize, User, UserKey};

pub struct AuthorizationEvent<P: Protocolize>(pub UserKey, pub P);
pub struct ConnectionEvent(pub UserKey);
pub struct DisconnectionEvent(pub UserKey, pub User);
pub struct MessageEvent<P: Protocolize>(pub UserKey, pub P);
pub struct MessageEntityEvent<P: Protocolize>(pub UserKey, pub Entity, pub P);
