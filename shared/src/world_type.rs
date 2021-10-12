use std::any::TypeId;

use naia_socket_shared::Ref;

use super::{
    entity_type::EntityType,
    protocol_type::ProtocolType,
    replicate::{ImplRef, Replicate},
};
use crate::ProtocolRefExtractor;

/// Structures that implement the WorldMutType trait will be able to be loaded
/// into the Server at which point the Server will use this interface to keep
/// the WorldMutType in-sync with it's own Entities/Components
pub trait WorldRefType<P: ProtocolType, K: EntityType> {
    // Entities

    /// check whether entity exists
    fn has_entity(&self, entity_key: &K) -> bool;
    /// get a list of all entities in the World
    fn entities(&self) -> Vec<K>;

    // Components

    /// check whether entity contains component
    fn has_component<R: Replicate<P>>(&self, entity_key: &K) -> bool;
    /// check whether entity contains component, dynamically
    fn has_component_of_type(&self, entity_key: &K, type_id: &TypeId) -> bool;
    /// gets an entity's component
    fn get_component<R: Replicate<P>>(&self, entity_key: &K) -> Option<Ref<R>>;
    /// gets an entity's component, dynamically
    fn get_component_from_type(&self, entity_key: &K, type_id: &TypeId) -> Option<P>;
}

/// Structures that implement the WorldMutType trait will be able to be loaded
/// into the Server at which point the Server will use this interface to keep
/// the WorldMutType in-sync with it's own Entities/Components
pub trait WorldMutType<P: ProtocolType, K: EntityType>:
    WorldRefType<P, K> + ProtocolRefExtractor<P, K>
{
    // Entities

    /// spawn an entity
    fn spawn_entity(&mut self) -> K;
    /// despawn an entity
    fn despawn_entity(&mut self, entity_key: &K);

    // Components

    /// gets all of an entity's components, as a Protocol
    fn get_components(&mut self, entity_key: &K) -> Vec<P>;
    /// insert a component
    fn insert_component<I: ImplRef<P>>(&mut self, entity_key: &K, component_ref: I);
    /// remove a component by type
    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &K);
    /// remove a component by type
    fn remove_component_by_type(&mut self, entity_key: &K, type_id: &TypeId);
}
