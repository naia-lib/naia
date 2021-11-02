use naia_shared::ProtocolType;

use super::owned_entity::OwnedEntity;

#[derive(Debug, Clone)]
pub enum EntityAction<P: ProtocolType, E: Copy> {
    SpawnEntity(E, Vec<P::Kind>),
    DespawnEntity(E),
    OwnEntity(OwnedEntity<E>),
    DisownEntity(OwnedEntity<E>),
    RewindEntity(OwnedEntity<E>),
    InsertComponent(E, P::Kind),
    UpdateComponent(E, P::Kind),
    RemoveComponent(E, P),
}
