use naia_shared::{LocalEntityKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum EntityAction<P: ProtocolType> {
    CreateEntity(LocalEntityKey, Vec<P>),
    DeleteEntity(LocalEntityKey),
    AssignPawn(LocalEntityKey),
    UnassignPawn(LocalEntityKey),
    ResetPawn(LocalEntityKey),
    AddComponent(LocalEntityKey, P),
    RemoveComponent(LocalEntityKey, P),
    UpdateComponent(LocalEntityKey, P),
}
