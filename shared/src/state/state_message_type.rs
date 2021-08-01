/// Enum used as a shared network protocol, representing various message types
/// related to States/Entities/Pawns/Components
#[derive(Copy, Clone)]
#[repr(u8)]
pub enum StateMessageType {
    /// Message indicating an State to be created
    CreateState = 0,
    /// Message indicating an State to be updated
    UpdateState,
    /// Message indicating an State to be deleted
    DeleteState,
    /// Message indicating an State to be assigned as a Pawn
    AssignPawn,
    /// Message indicating an State to be unassigned as a Pawn
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

impl StateMessageType {
    /// Converts the message type to u8
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    /// Gets an StateMessageType from a u8
    #[allow(unsafe_code)]
    pub fn from_u8(v: u8) -> Self {
        if v >= StateMessageType::Unknown as u8 {
            return StateMessageType::Unknown;
        }
        let z: StateMessageType = unsafe { ::std::mem::transmute(v) };
        z
    }
}

