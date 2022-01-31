use bevy::ecs::entity::Entity;

use naia_client::{OwnedEntity as NaiaOwnedEntity, Protocolize};

pub type OwnedEntity = NaiaOwnedEntity<Entity>;

pub struct SpawnEntityEvent<P: Protocolize>(pub Entity, pub Vec<P::Kind>);
pub struct DespawnEntityEvent(pub Entity);
pub struct OwnEntityEvent(pub OwnedEntity);
pub struct DisownEntityEvent(pub OwnedEntity);
pub struct RewindEntityEvent(pub OwnedEntity);
pub struct InsertComponentEvent<P: Protocolize>(pub Entity, pub P::Kind);
pub struct UpdateComponentEvent<P: Protocolize>(pub Entity, pub P::Kind);
pub struct RemoveComponentEvent<P: Protocolize>(pub Entity, pub P);
pub struct MessageEvent<P: Protocolize>(pub P);
pub struct NewCommandEvent<P: Protocolize>(pub OwnedEntity, pub P);
pub struct ReplayCommandEvent<P: Protocolize>(pub OwnedEntity, pub P);
