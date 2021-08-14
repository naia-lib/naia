use naia_shared::{LocalObjectKey, LocalEntityKey, LocalComponentKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum ClientReplicateMessage<U: ProtocolType> {
    CreateReplicate(LocalObjectKey),
    UpdateReplicate(LocalObjectKey),
    DeleteReplicate(LocalObjectKey, U),
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