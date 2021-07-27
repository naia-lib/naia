use naia_shared::{LocalActorKey, LocalEntityKey, LocalComponentKey, ActorType};

#[derive(Debug, Clone)]
pub enum ClientActorMessage<U: ActorType> {
    CreateActor(LocalActorKey),
    UpdateActor(LocalActorKey),
    DeleteActor(LocalActorKey, U),
    AssignPawn(LocalActorKey),
    UnassignPawn(LocalActorKey),
    ResetPawn(LocalActorKey),
    CreateEntity(LocalEntityKey, Vec<LocalComponentKey>),
    DeleteEntity(LocalEntityKey),
    AssignPawnEntity(LocalEntityKey),
    UnassignPawnEntity(LocalEntityKey),
    ResetPawnEntity(LocalEntityKey),
    AddComponent(LocalEntityKey, LocalComponentKey),
    UpdateComponent(LocalEntityKey, LocalComponentKey),
    RemoveComponent(LocalEntityKey, LocalComponentKey, U),
}