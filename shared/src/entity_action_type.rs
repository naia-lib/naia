/// Enum used as a shared network protocol, representing various message types
/// related to Entities/Pawns/Components
#[derive(Copy, Clone)]
#[repr(u8)]
pub enum EntityActionType {
    /// Action indicating a Replica to be updated
    UpdateComponent = 0,
    /// Action indicating a Replica to be deleted
    RemoveComponent,
    /// Action indicating an Entity to be created
    SpawnEntity,
    /// Action indicating an Entity to be deleted
    DespawnEntity,
    /// Action indicating an Entity to be assigned as a Pawn
    AssignPawn,
    /// Action indicating an Entity to be unassigned as a Pawn
    UnassignPawn,
    /// Action indicating a Component to be added to an Entity
    InsertComponent,
    /// Unknown / Undefined message, should always be last variant in this list
    Unknown,
}

impl EntityActionType {
    /// Converts the action type to u8
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    /// Gets an EntityActionType from a u8
    #[allow(unsafe_code)]
    pub fn from_u8(v: u8) -> Self {
        if v >= EntityActionType::Unknown as u8 {
            return EntityActionType::Unknown;
        }
        let z: EntityActionType = unsafe { ::std::mem::transmute(v) };
        z
    }
}
