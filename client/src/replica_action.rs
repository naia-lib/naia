use naia_shared::{LocalEntityKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum ReplicaAction<U: ProtocolType> {
//    CreateObject(LocalObjectKey),
//    UpdateObject(LocalObjectKey),
//    DeleteObject(LocalObjectKey, U),
//    AssignPawn(LocalObjectKey),
//    UnassignPawn(LocalObjectKey),
//    ResetPawn(LocalObjectKey),
    CreateEntity(LocalEntityKey, Vec<U>),
    DeleteEntity(LocalEntityKey),
    AssignPawnEntity(LocalEntityKey),
    UnassignPawnEntity(LocalEntityKey),
    ResetPawnEntity(LocalEntityKey),
    AddComponent(LocalEntityKey, U),
    UpdateComponent(LocalEntityKey, U),
    RemoveComponent(LocalEntityKey, U),
}
