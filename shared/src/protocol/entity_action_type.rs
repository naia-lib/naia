use crate::{derive_serde, serde};

// Enum used as a shared network protocol, representing various message types
// related to Entities/Components
#[derive(Copy)]
#[derive_serde]
pub enum EntityActionType {
    // Action indicating an Entity to be created
    SpawnEntity,
    // Action indicating an Entity to be deleted
    DespawnEntity,
    // Action indicating a Component to be added to an Entity
    InsertComponent,
    // Action indicating a Component to be deleted
    RemoveComponent,
    // Action indicating nothing should be done, necessary to track outdated Action Packets
    Noop,
}
