use naia_shared::LocalActorKey;

#[derive(Debug, Clone)]
pub enum ClientActorMessage {
    Create(LocalActorKey),
    Update(LocalActorKey),
    Delete(LocalActorKey),
    AssignPawn(LocalActorKey),
    UnassignPawn(LocalActorKey),
}
