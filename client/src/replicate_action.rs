use naia_shared::{LocalReplicateKey, LocalEntityKey, LocalComponentKey, ProtocolType};

#[derive(Debug, Clone)]
pub enum ReplicateAction<U: ProtocolType> {
    CreateReplicate(LocalReplicateKey),
    UpdateReplicate(LocalReplicateKey),
    DeleteReplicate(LocalReplicateKey, U),
    AssignPawn(LocalReplicateKey),
    UnassignPawn(LocalReplicateKey),
    ResetPawn(LocalReplicateKey),
    CreateEntity(LocalEntityKey, Vec<LocalComponentKey>),
    DeleteEntity(LocalEntityKey),
    AssignPawnEntity(LocalEntityKey),
    UnassignPawnEntity(LocalEntityKey),
    ResetPawnEntity(LocalEntityKey),
    AddComponent(LocalEntityKey, LocalComponentKey),
    UpdateComponent(LocalEntityKey, LocalComponentKey),
    RemoveComponent(LocalEntityKey, LocalComponentKey, U),
}