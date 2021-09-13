use naia_shared::{LocalEntityKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum EntityAction<P: ProtocolType> {
    SpawnEntity(LocalEntityKey, Vec<P>),
    DespawnEntity(LocalEntityKey),
    OwnEntity(LocalEntityKey),
    DisownEntity(LocalEntityKey),
    RewindEntity(LocalEntityKey),
    InsertComponent(LocalEntityKey, P),
    RemoveComponent(LocalEntityKey, P),
    UpdateComponent(LocalEntityKey, P),
}
