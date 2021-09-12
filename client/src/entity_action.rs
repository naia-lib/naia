use naia_shared::{LocalEntityKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum EntityAction<U: ProtocolType> {
    CreateEntity(LocalEntityKey, Vec<U>),
    DeleteEntity(LocalEntityKey),
    AssignPawn(LocalEntityKey),
    UnassignPawn(LocalEntityKey),
    ResetPawn(LocalEntityKey),
    AddComponent(LocalEntityKey, U),
    RemoveComponent(LocalEntityKey, U),
    UpdateComponent(LocalEntityKey, U),
}
