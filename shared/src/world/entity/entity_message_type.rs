use naia_serde::SerdeInternal;

use crate::{ComponentKind, EntityMessage};

/// Wire discriminant identifying the kind of entity/component event carried in an `EntityMessage`.
#[derive(Copy, PartialEq, Clone, SerdeInternal, Debug)]
pub enum EntityMessageType {
    /// Entity is to be created.
    Spawn,
    /// Entity is to be created with an initial set of components (coalesced spawn).
    SpawnWithComponents,
    /// Entity is to be deleted.
    Despawn,
    /// A component is to be added to an entity.
    InsertComponent,
    /// A component is to be removed from an entity.
    RemoveComponent,
    /// Entity is to be published (made visible to other users).
    Publish,
    /// Entity publication is to be retracted.
    Unpublish,
    /// Authority delegation is to be enabled for an entity.
    EnableDelegation,
    /// Authority delegation is to be disabled for an entity.
    DisableDelegation,
    /// Authority status for an entity is being updated.
    SetAuthority,

    /// No-operation placeholder.
    Noop,

    /// Client requests authority over an entity.
    RequestAuthority,
    /// Client releases authority over an entity.
    ReleaseAuthority,
    /// Client acknowledges that delegation has been enabled.
    EnableDelegationResponse,
    /// Server notifies that an entity has migrated.
    MigrateResponse,
}

impl EntityMessageType {
    /// Builds an `EntityMessage<()>` for component-bearing types, attaching `component_kind`. Panics for other variants.
    pub fn with_component_kind(&self, component_kind: &ComponentKind) -> EntityMessage<()> {
        match self {
            Self::InsertComponent => EntityMessage::InsertComponent((), *component_kind),
            Self::RemoveComponent => EntityMessage::RemoveComponent((), *component_kind),
            t => panic!("Cannot apply component kind to message type: {:?}", t),
        }
    }
}
