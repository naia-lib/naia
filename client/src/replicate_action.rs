use naia_shared::{LocalObjectKey, LocalEntityKey, LocalComponentKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum ReplicateAction<U: ProtocolType> {
    CreateObject(LocalObjectKey),
    UpdateObject(LocalObjectKey),
    DeleteObject(LocalObjectKey, U),
    AssignPawn(LocalObjectKey),
    UnassignPawn(LocalObjectKey),
    ResetPawn(LocalObjectKey),
    CreateEntity(LocalEntityKey, Vec<LocalComponentKey>),
    DeleteEntity(LocalEntityKey),
    AssignPawnEntity(LocalEntityKey),
    UnassignPawnEntity(LocalEntityKey),
    ResetPawnEntity(LocalEntityKey),
    AddComponent(LocalEntityKey, LocalComponentKey),
    UpdateComponent(LocalEntityKey, LocalComponentKey),
    RemoveComponent(LocalEntityKey, LocalComponentKey, U),
}