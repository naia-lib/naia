use bevy::ecs::entity::Entity;

use naia_server::{ProtocolType, UserKey, UserRecord};

pub struct AuthorizationEvent<P: ProtocolType>(pub UserKey, pub P);
pub struct ConnectionEvent(pub UserKey);
pub struct DisconnectionEvent(pub UserKey, pub UserRecord);
pub struct MessageEvent<P: ProtocolType>(pub UserKey, pub P);
pub struct CommandEvent<P: ProtocolType>(pub UserKey, pub Entity, pub P);
