use naia_shared::{LocalActorKey, LocalEntityKey};

#[derive(Debug, Clone)]
pub enum ClientActorMessage {
    CreateActor(LocalActorKey),
    UpdateActor(LocalActorKey),
    DeleteActor(LocalActorKey),
    AssignPawn(LocalActorKey),
    UnassignPawn(LocalActorKey),
    ResetPawn(LocalActorKey),
    CreateEntity(LocalEntityKey),
    DeleteEntity(LocalEntityKey),
    AssignPawnEntity(LocalEntityKey),
    UnassignPawnEntity(LocalEntityKey),
    ResetPawnEntity(LocalEntityKey),
}
