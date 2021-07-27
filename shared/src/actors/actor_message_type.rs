/// Enum used as a shared network protocol, representing various message types
/// related to Actors/Entities/Pawns/Components
#[derive(Copy, Clone)]
#[repr(u8)]
pub enum ActorMessageType {
    /// Message indicating an Actor to be created
    CreateActor = 0,
    /// Message indicating an Actor to be updated
    UpdateActor,
    /// Message indicating an Actor to be deleted
    DeleteActor,
    /// Message indicating an Actor to be assigned as a Pawn
    AssignPawn,
    /// Message indicating an Actor to be unassigned as a Pawn
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
    //AddComponent,
    /// Unknown / Undefined message, should always be last variant in this list
    Unknown
}

impl ActorMessageType {
    /// Converts the message type to u8
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    /// Gets an ActorMessageType from a u8
    #[allow(unsafe_code)]
    pub fn from_u8(v: u8) -> Self {
        if v >= ActorMessageType::Unknown as u8 {
            return ActorMessageType::Unknown;
        }
        let z: ActorMessageType = unsafe { ::std::mem::transmute(v) };
        z
    }
}

