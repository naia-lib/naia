use naia_shared::{LocalObjectKey, LocalEntityKey, LocalComponentKey, StateType};

#[derive(Debug, Clone)]
pub enum ClientStateMessage<U: StateType> {
    CreateState(LocalObjectKey),
    UpdateState(LocalObjectKey),
    DeleteState(LocalObjectKey, U),
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