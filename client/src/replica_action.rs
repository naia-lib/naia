use naia_shared::{LocalComponentKey, LocalEntityKey, LocalObjectKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum ReplicaAction<U: ProtocolType> {
//    CreateObject(LocalObjectKey),
//    UpdateObject(LocalObjectKey),
//    DeleteObject(LocalObjectKey, U),
//    AssignPawn(LocalObjectKey),
//    UnassignPawn(LocalObjectKey),
//    ResetPawn(LocalObjectKey),
    CreateEntity(LocalEntityKey, Vec<LocalComponentKey>),
    DeleteEntity(LocalEntityKey),
    AssignPawnEntity(LocalEntityKey),
    UnassignPawnEntity(LocalEntityKey),
    ResetPawnEntity(LocalEntityKey),
    AddComponent(LocalEntityKey, LocalComponentKey),
    UpdateComponent(LocalEntityKey, LocalComponentKey),
    RemoveComponent(LocalEntityKey, LocalComponentKey, U),
}
