use naia_shared::{LocalEntityKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum ReplicaAction<U: ProtocolType> {
    CreateEntity(LocalEntityKey, Vec<U>),
    DeleteEntity(LocalEntityKey),
    AssignPawnEntity(LocalEntityKey),
    UnassignPawnEntity(LocalEntityKey),
    ResetPawnEntity(LocalEntityKey),
    AddComponent(LocalEntityKey, U),
    UpdateComponent(LocalEntityKey, U),
    RemoveComponent(LocalEntityKey, U),
}
