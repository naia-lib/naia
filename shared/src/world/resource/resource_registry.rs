use std::{any::TypeId, collections::HashMap};

use crate::GlobalEntity;

/// Per-`World` bidirectional map between Resource `TypeId` and the hidden
/// `GlobalEntity` carrying that resource as a single component.
///
/// Maintained on both sides:
///
/// - Sender (`HostWorldManager`): inserted at `insert_resource::<R>(...)`
///   time, removed at `remove_resource::<R>()` time.
/// - Receiver (`RemoteWorldManager`): inserted when an incoming
///   `SpawnWithComponents` carries a component whose kind is registered
///   in `protocol.resource_kinds`. Removed on entity despawn.
///
/// Lookups are O(1) in both directions:
/// - `entity_for(TypeId)` → "where is the hidden entity for resource R?"
/// - `type_for(GlobalEntity)` → "is this entity a resource, and which
///   one?" (used to suppress the entity from user-visible scope/event
///   streams).
#[derive(Clone, Debug, Default)]
pub struct ResourceRegistry {
    by_type: HashMap<TypeId, GlobalEntity>,
    by_entity: HashMap<GlobalEntity, TypeId>,
}

/// Error returned when attempting to insert a resource type that is
/// already present in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceAlreadyExists;

impl std::fmt::Display for ResourceAlreadyExists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "resource of this type is already registered")
    }
}

impl std::error::Error for ResourceAlreadyExists {}

impl ResourceRegistry {
    /// Creates an empty `ResourceRegistry`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a (TypeId, GlobalEntity) pair. Fails with
    /// `ResourceAlreadyExists` if the TypeId is already registered (the
    /// `commands.replicate_resource` API surface treats this as an error
    /// per D14/risk-register).
    pub fn insert<R: 'static>(
        &mut self,
        entity: GlobalEntity,
    ) -> Result<(), ResourceAlreadyExists> {
        let type_id = TypeId::of::<R>();
        if self.by_type.contains_key(&type_id) {
            return Err(ResourceAlreadyExists);
        }
        self.by_type.insert(type_id, entity);
        self.by_entity.insert(entity, type_id);
        Ok(())
    }

    /// Receiver-side variant: insert by raw TypeId (the receiver derives
    /// the TypeId from the incoming `ComponentKind` via the
    /// `ResourceKinds` registration). Idempotent if the same pair is
    /// already present (e.g. spawn-after-spawn replay), returns error
    /// otherwise.
    pub fn insert_raw(
        &mut self,
        type_id: TypeId,
        entity: GlobalEntity,
    ) -> Result<(), ResourceAlreadyExists> {
        if let Some(existing) = self.by_type.get(&type_id) {
            if *existing == entity {
                return Ok(());
            }
            return Err(ResourceAlreadyExists);
        }
        self.by_type.insert(type_id, entity);
        self.by_entity.insert(entity, type_id);
        Ok(())
    }

    /// Remove a resource by type. Returns the removed entity if present.
    pub fn remove<R: 'static>(&mut self) -> Option<GlobalEntity> {
        let type_id = TypeId::of::<R>();
        let entity = self.by_type.remove(&type_id)?;
        self.by_entity.remove(&entity);
        Some(entity)
    }

    /// Receiver-side: remove by entity (used when an incoming Despawn
    /// for a resource entity arrives).
    pub fn remove_by_entity(&mut self, entity: &GlobalEntity) -> Option<TypeId> {
        let type_id = self.by_entity.remove(entity)?;
        self.by_type.remove(&type_id);
        Some(type_id)
    }

    /// O(1): "where is the hidden entity for resource `R`?"
    pub fn entity_for<R: 'static>(&self) -> Option<GlobalEntity> {
        self.by_type.get(&TypeId::of::<R>()).copied()
    }

    /// O(1) raw-TypeId variant.
    pub fn entity_for_raw(&self, type_id: &TypeId) -> Option<GlobalEntity> {
        self.by_type.get(type_id).copied()
    }

    /// O(1): "is this entity a resource entity, and if so which type?"
    pub fn type_for(&self, entity: &GlobalEntity) -> Option<TypeId> {
        self.by_entity.get(entity).copied()
    }

    /// O(1): "is this entity a resource entity?"
    pub fn is_resource_entity(&self, entity: &GlobalEntity) -> bool {
        self.by_entity.contains_key(entity)
    }

    /// Returns the number of registered resources.
    pub fn len(&self) -> usize {
        self.by_type.len()
    }

    /// Returns `true` if no resources have been registered.
    pub fn is_empty(&self) -> bool {
        self.by_type.is_empty()
    }

    /// Iterate over all `(TypeId, GlobalEntity)` pairs. Used by the
    /// scope resolver to auto-include resource entities in every user's
    /// scope.
    pub fn iter(&self) -> impl Iterator<Item = (&TypeId, &GlobalEntity)> {
        self.by_type.iter()
    }

    /// Iterate over just the resource entities (for scope auto-inclusion).
    pub fn entities(&self) -> impl Iterator<Item = &GlobalEntity> {
        self.by_type.values()
    }
}

// Behavioral tests using real `Replicate` types live in the integration
// suite; this module's tests exercise the raw TypeId/GlobalEntity
// mechanics directly.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::BigMapKey;

    fn ge(n: u64) -> GlobalEntity {
        GlobalEntity::from_u64(n)
    }

    fn tya() -> TypeId {
        struct A;
        TypeId::of::<A>()
    }
    fn tyb() -> TypeId {
        struct B;
        TypeId::of::<B>()
    }

    #[test]
    fn insert_raw_and_lookup_both_directions() {
        let mut r = ResourceRegistry::new();
        let e = ge(1);
        r.insert_raw(tya(), e).unwrap();

        assert_eq!(r.entity_for_raw(&tya()), Some(e));
        assert_eq!(r.type_for(&e), Some(tya()));
        assert!(r.is_resource_entity(&e));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn double_insert_same_type_distinct_entity_errors() {
        let mut r = ResourceRegistry::new();
        r.insert_raw(tya(), ge(1)).unwrap();
        assert_eq!(r.insert_raw(tya(), ge(2)), Err(ResourceAlreadyExists));
        assert_eq!(r.entity_for_raw(&tya()), Some(ge(1)));
    }

    #[test]
    fn insert_raw_idempotent_for_identical_pair() {
        let mut r = ResourceRegistry::new();
        let e = ge(7);
        r.insert_raw(tya(), e).unwrap();
        r.insert_raw(tya(), e).unwrap(); // same pair: no-op
        assert_eq!(r.insert_raw(tya(), ge(8)), Err(ResourceAlreadyExists));
    }

    #[test]
    fn remove_by_entity_clears_both_indices() {
        let mut r = ResourceRegistry::new();
        let e = ge(4);
        r.insert_raw(tya(), e).unwrap();
        let ty = r.remove_by_entity(&e);
        assert_eq!(ty, Some(tya()));
        assert!(r.is_empty());
        assert!(!r.is_resource_entity(&e));
    }

    #[test]
    fn multi_type_isolation() {
        let mut r = ResourceRegistry::new();
        r.insert_raw(tya(), ge(1)).unwrap();
        r.insert_raw(tyb(), ge(2)).unwrap();

        assert_eq!(r.entity_for_raw(&tya()), Some(ge(1)));
        assert_eq!(r.entity_for_raw(&tyb()), Some(ge(2)));
        r.remove_by_entity(&ge(1));
        assert_eq!(r.entity_for_raw(&tya()), None);
        assert_eq!(r.entity_for_raw(&tyb()), Some(ge(2)));
    }
}
