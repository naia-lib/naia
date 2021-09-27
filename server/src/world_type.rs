use std::any::TypeId;

use naia_shared::{ImplRef, ProtocolType, Ref, Replicate};

use super::keys::KeyType;

/// Structures that implement the WorldType trait will be able to be loaded into
/// the Server at which point the Server will use this interface to keep the
/// WorldType in-sync with it's own Entities/Components
pub trait WorldType<P: ProtocolType> {
    /// Key to be used to access Entities in the given World
    type EntityKey: KeyType;

    // Entities

    /// check whether entity exists
    fn has_entity(&self, entity_key: &Self::EntityKey) -> bool;
    /// get a list of all entities in the World
    fn entities(&self) -> Vec<Self::EntityKey>;
    /// spawn an entity
    fn spawn_entity(&mut self) -> Self::EntityKey;
    /// despawn an entity
    fn despawn_entity(&mut self, entity_key: &Self::EntityKey);

    // Components

    /// check whether entity contains component
    fn has_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> bool;
    /// check whether entity contains component, dynamically
    fn has_component_of_type(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> bool;
    /// gets an entity's component
    fn get_component<R: Replicate<P>>(&self, entity_key: &Self::EntityKey) -> Option<Ref<R>>;
    /// gets an entity's component, dynamically
    fn get_component_from_type(&self, entity_key: &Self::EntityKey, type_id: &TypeId) -> Option<P>;
    /// gets all of an entity's components, as a Protocol
    fn get_components(&self, entity_key: &Self::EntityKey) -> Vec<P>;
    /// insert a component
    fn insert_component<R: ImplRef<P>>(&mut self, entity_key: &Self::EntityKey, component_ref: R);
    /// remove a component
    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &Self::EntityKey);
}
