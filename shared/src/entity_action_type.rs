use crate::{derive_serde, serde};

// Enum used as a shared network protocol, representing various message types
// related to Entities/Components
#[derive(Copy)]
#[derive_serde]
pub enum EntityActionType {
    // Action indicating a Component to be updated
    UpdateComponent,
    // Action indicating a Component to be deleted
    RemoveComponent,
    // Action indicating an Entity to be created
    SpawnEntity,
    // Action indicating an Entity to be deleted
    DespawnEntity,
    // Action indicating a Message to be sent to an Entity
    MessageEntity,
    // Action indicating a Component to be added to an Entity
    InsertComponent,
}
