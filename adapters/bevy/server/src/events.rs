use bevy::ecs::entity::Entity;

use naia_server::{ProtocolType, User, UserKey};

pub struct AuthorizationEvent<P: ProtocolType>(pub UserKey, pub P);
pub struct ConnectionEvent(pub UserKey);
pub struct DisconnectionEvent(pub UserKey, pub User);
pub struct MessageEvent<P: ProtocolType>(pub UserKey, pub P);
pub struct CommandEvent<P: ProtocolType>(pub UserKey, pub Entity, pub P);
