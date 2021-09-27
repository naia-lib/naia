use std::any::TypeId;

use naia_shared::{ImplRef, ProtocolType, Ref, Replicate};

use super::keys::{KeyType, ComponentKey};

/// Structures that implement the WorldType trait will be able to be loaded into
/// the Server at which point the Server will use this interface to keep the
/// WorldType in-sync with it's own Entities/Components
pub trait WorldType<P: ProtocolType> {
    /// Key to be used to access Entities in the given World
    type EntityKey: KeyType;

    /// spawn an entity
    fn spawn_entity(&mut self) -> Self::EntityKey;
    /// despawn an entity
    fn despawn_entity(&mut self, entity_key: &Self::EntityKey);
    /// check whether entity exists
    fn has_entity(&self, entity_key: &Self::EntityKey) -> bool;
    /// check whether entity contains component
    fn has_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> bool;
    /// check whether entity contains component, dynamically
    fn has_component_dynamic(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> bool;
    /// gets an entity's component
    fn get_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> Option<Ref<R>>;
    /// gets an entity's component, dynamically
    fn get_component_dynamic(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> Option<P>;
    /// gets an entity's component, from a ComponentKey
    fn get_component_from_key(&self, component_key: &ComponentKey<Self::EntityKey>) -> Option<P>;
    /// gets all of an entity's components, as a Protocol
    fn get_components(&self, entity_key: &Self::EntityKey) -> Vec<P>;
    /// insert a component
    fn insert_component<R: ImplRef<P>>(
        &mut self,
        entity_key: &Self::EntityKey,
        component_ref: R,
    );
    /// remove a component
    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &Self::EntityKey);
}
