use naia_serde::SerdeInternal;

use crate::{ComponentKind, EntityMessage};

// Enum used as a shared network protocol, representing various message types
// related to Entities/Components
#[derive(Copy, PartialEq, Clone, SerdeInternal, Debug)]
pub enum EntityMessageType {
    // Action indicating an Entity to be created
    Spawn,
    // Action indicating an Entity to be created with initial components (coalesced spawn)
    SpawnWithComponents,
    // Action indicating an Entity to be deleted
    Despawn,
    // Action indicating a Component to be added to an Entity
    InsertComponent,
    // Action indicating a Component to be deleted
    RemoveComponent,
    // Action indicating an Entity to be published
    Publish,
    // Action indicating an Entity to be unpublished
    Unpublish,
    // Action indicating delegation to be enabled for an Entity
    EnableDelegation,
    // Action indicating delegation to be disabled for an Entity
    DisableDelegation,
    // Action updating authority status for an Entity
    SetAuthority,

    // Action indicating a non-operation
    Noop,

    // Action requesting authority for an Entity
    RequestAuthority,
    // Action releasing authority for an Entity
    ReleaseAuthority,
    // Action indicating delegation enable response
    EnableDelegationResponse,
    // Action responding to entity migration
    MigrateResponse,
}

impl EntityMessageType {
    pub fn with_component_kind(&self, component_kind: &ComponentKind) -> EntityMessage<()> {
        match self {
            Self::InsertComponent => EntityMessage::InsertComponent((), *component_kind),
            Self::RemoveComponent => EntityMessage::RemoveComponent((), *component_kind),
            t => panic!("Cannot apply component kind to message type: {:?}", t),
        }
    }
}
