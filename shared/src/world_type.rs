use super::{
    entity_type::EntityType,
    protocol_type::ProtocolType,
    impls::{Replicate, ReplicateEq},
};

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
    fn has_component_of_kind(&self, entity_key: &K, component_kind: &P::Kind) -> bool;
    /// gets an entity's component
    fn get_component<R: Replicate<P>>(&self, entity_key: &K) -> Option<&R>;
    /// gets an entity's component, dynamically
    fn get_component_of_kind(&self, entity_key: &K, component_kind: &P::Kind) -> Option<P>;
}

/// Structures that implement the WorldMutType trait will be able to be loaded
/// into the Server at which point the Server will use this interface to keep
/// the WorldMutType in-sync with it's own Entities/Components
pub trait WorldMutType<P: ProtocolType, K: EntityType>: WorldRefType<P, K>
{
    // Entities

    /// spawn an entity
    fn spawn_entity(&mut self) -> K;
    /// despawn an entity
    fn despawn_entity(&mut self, entity_key: &K);

    // Components
    /// gets an entity's component
    fn get_component_mut<R: Replicate<P>>(&mut self, entity_key: &K) -> Option<&mut R>;
    /// gets all of an entity's components, as a Protocol
    fn get_components(&mut self, entity_key: &K) -> Vec<P>;
    /// insert a component
    fn insert_component<R: Replicate<P>>(&mut self, entity_key: &K, component_ref: R);
    /// remove a component
    fn remove_component<R: ReplicateEq<P>>(&mut self, entity_key: &K) -> Option<R>;
    /// remove a component by kind
    fn remove_component_of_kind(&mut self, entity_key: &K, component_kind: &P::Kind) -> Option<P>;
}
