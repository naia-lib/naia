use naia_serde::SerdeInternal;

// Enum used as a shared network protocol, representing various message types
// related to Entities/Components
#[derive(Copy, PartialEq, Clone, SerdeInternal)]
pub enum EntityActionType {
    // Action indicating an Entity to be created
    SpawnEntity,
    // Action indicating an Entity to be deleted
    DespawnEntity,
    // Action indicating a Component to be added to an Entity
    InsertComponent,
    // Action indicating a Component to be deleted
    RemoveComponent,
    // Action indicating a non-operation
    Noop,
}
