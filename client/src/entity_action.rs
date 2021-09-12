use naia_shared::{LocalEntityKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum EntityAction<P: ProtocolType> {
    SpawnEntity(LocalEntityKey, Vec<P>),
    DespawnEntity(LocalEntityKey),
    AssignEntity(LocalEntityKey),
    UnassignEntity(LocalEntityKey),
    RewindEntity(LocalEntityKey),
    InsertComponent(LocalEntityKey, P),
    RemoveComponent(LocalEntityKey, P),
    UpdateComponent(LocalEntityKey, P),
}
