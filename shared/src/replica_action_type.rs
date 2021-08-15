/// Enum used as a shared network protocol, representing various message types
/// related to Objects/Entities/Pawns/Components
#[derive(Copy, Clone)]
#[repr(u8)]
pub enum ReplicaActionType {
    /// Action indicating an Object to be created
    CreateObject = 0,
    /// Action indicating a Replica to be updated
    UpdateReplica,
    /// Action indicating a Replica to be deleted
    DeleteReplica,
    /// Action indicating an Object to be assigned as a Pawn
    AssignPawn,
    /// Action indicating an Object to be unassigned as a Pawn
    UnassignPawn,
    /// Action indicating a Pawn to be updated
    UpdatePawn,
    /// Action indicating an Entity to be created
    CreateEntity,
    /// Action indicating an Entity to be deleted
    DeleteEntity,
    /// Action indicating an Entity to be assigned as a Pawn
    AssignPawnEntity,
    /// Action indicating an Entity to be unassigned as a Pawn
    UnassignPawnEntity,
    /// Action indicating a Component to be added to an Entity
    AddComponent,
    /// Unknown / Undefined message, should always be last variant in this list
    Unknown,
}

impl ReplicaActionType {
    /// Converts the action type to u8
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    /// Gets an ReplicaActionType from a u8
    #[allow(unsafe_code)]
    pub fn from_u8(v: u8) -> Self {
        if v >= ReplicaActionType::Unknown as u8 {
            return ReplicaActionType::Unknown;
        }
        let z: ReplicaActionType = unsafe { ::std::mem::transmute(v) };
        z
    }
}
