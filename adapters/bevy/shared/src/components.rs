use std::any::{Any, TypeId};
use std::collections::HashMap;

use bevy_ecs::{component::Component, entity::Entity, resource::Resource};

/// Marker component tagging an entity as host-owned by host tag type `T`,
/// recording `T`'s `TypeId` so the owning host tier can be recovered later.
#[derive(Component, Clone, Copy)]
pub struct HostOwned {
    type_id: TypeId,
}

impl HostOwned {
    /// Creates a marker recording host tag type `T`.
    pub fn new<T: Any>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
        }
    }

    /// Returns the host tag `TypeId` this marker was created with.
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }
}

/// Resource mapping each host-owned entity to its [`HostOwned`] marker, kept
/// in sync by [`crate::on_host_owned_added`] and consumed by despawn
/// detection.
#[derive(Resource, Default)]
pub struct HostOwnedMap {
    map: HashMap<Entity, HostOwned>,
}

impl HostOwnedMap {
    /// Records `host_owned` for `entity`, replacing any existing entry.
    pub fn insert(&mut self, entity: Entity, host_owned: HostOwned) {
        self.map.insert(entity, host_owned);
    }

    /// Removes and returns the `HostOwned` marker recorded for `entity`, if any.
    pub fn remove(&mut self, entity: &Entity) -> Option<HostOwned> {
        self.map.remove(entity)
    }
}
