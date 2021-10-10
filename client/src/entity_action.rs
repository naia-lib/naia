use naia_shared::{ProtocolType, EntityType};

#[derive(Debug, Clone)]
pub enum EntityAction<P: ProtocolType, K: EntityType> {
    SpawnEntity(K, Vec<P>),
    DespawnEntity(K),
    OwnEntity(K),
    DisownEntity(K),
    RewindEntity(K),
    InsertComponent(K, P),
    UpdateComponent(K, P),
    RemoveComponent(K, P),
}
