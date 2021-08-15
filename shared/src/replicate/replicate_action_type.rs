/// Enum used as a shared network protocol, representing various message types
/// related to Replicates/Entities/Pawns/Components
#[derive(Copy, Clone)]
#[repr(u8)]
pub enum ReplicateActionType {
    /// Message indicating an Replicate to be created
    CreateObject = 0,
    /// Message indicating an Replicate to be updated
    UpdateObject,
    /// Message indicating an Replicate to be deleted
    DeleteObject,
    /// Message indicating an Replicate to be assigned as a Pawn
    AssignPawn,
    /// Message indicating an Replicate to be unassigned as a Pawn
    UnassignPawn,
    /// Message indicating a Pawn to be updated
    UpdatePawn,
    /// Message indicating an Entity to be created
    CreateEntity,
    /// Message indicating an Entity to be deleted
    DeleteEntity,
    /// Message indicating an Entity to be assigned as a Pawn
    AssignPawnEntity,
    /// Message indicating an Entity to be unassigned as a Pawn
    UnassignPawnEntity,
    /// Message indicating a Component to be added to an Entity
    AddComponent,
    /// Unknown / Undefined message, should always be last variant in this list
    Unknown
}

impl ReplicateActionType {
    /// Converts the message type to u8
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    /// Gets an ReplicateActionType from a u8
    #[allow(unsafe_code)]
    pub fn from_u8(v: u8) -> Self {
        if v >= ReplicateActionType::Unknown as u8 {
            return ReplicateActionType::Unknown;
        }
        let z: ReplicateActionType = unsafe { ::std::mem::transmute(v) };
        z
    }
}

