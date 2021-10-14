//use std::collections::VecDeque;

use naia_client::{ProtocolType, OwnedEntity as NaiaOwnedEntity};

use naia_bevy_shared::Entity;

pub type OwnedEntity = NaiaOwnedEntity<Entity>;

pub struct SpawnEntityEvent<P: ProtocolType>(pub Entity, pub Vec<P>);
pub struct DespawnEntityEvent(pub Entity);
pub struct OwnEntityEvent(pub OwnedEntity);
pub struct DisownEntityEvent(pub OwnedEntity);
pub struct RewindEntityEvent(pub OwnedEntity);
pub struct InsertComponentEvent<P: ProtocolType>(pub Entity, pub P);
pub struct UpdateComponentEvent<P: ProtocolType>(pub Entity, pub P);
pub struct RemoveComponentEvent<P: ProtocolType>(pub Entity, pub P);
pub struct MessageEvent<P: ProtocolType>(pub P);
pub struct NewCommandEvent<P: ProtocolType>(pub OwnedEntity, pub P);
pub struct ReplayCommandEvent<P: ProtocolType>(pub OwnedEntity, pub P);