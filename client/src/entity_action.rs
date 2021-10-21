use naia_shared::{EntityType, ProtocolType};

use super::event::OwnedEntity;

#[derive(Debug, Clone)]
pub enum EntityAction<P: ProtocolType, E: EntityType> {
    SpawnEntity(E, Vec<P>),
    DespawnEntity(E),
    OwnEntity(OwnedEntity<E>),
    DisownEntity(OwnedEntity<E>),
    RewindEntity(OwnedEntity<E>),
    InsertComponent(E, P),
    UpdateComponent(E, P),
    RemoveComponent(E, P),
}
