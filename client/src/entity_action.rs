use naia_shared::{LocalEntity, ProtocolType};

#[derive(Debug, Clone)]
pub enum EntityAction<P: ProtocolType> {
    SpawnEntity(LocalEntity, Vec<P>),
    DespawnEntity(LocalEntity),
    OwnEntity(LocalEntity),
    DisownEntity(LocalEntity),
    RewindEntity(LocalEntity),
    InsertComponent(LocalEntity, P),
    UpdateComponent(LocalEntity, P),
    RemoveComponent(LocalEntity, P),
}
