use super::{
    component_ref::{ComponentDynMut, ComponentDynRef, ComponentMut, ComponentRef},
    entity_type::EntityType,
    protocol_type::{ProtocolInserter, ProtocolType},
    replicate::{Replicate, ReplicateSafe},
};

/// Structures that implement the WorldMutType trait will be able to be loaded
/// into the Server at which point the Server will use this interface to keep
/// the WorldMutType in-sync with it's own Entities/Components
pub trait WorldRefType<P: ProtocolType, E: EntityType> {
    // Entities
    /// check whether entity exists
    fn has_entity(&self, entity: &E) -> bool;
    /// get a list of all entities in the World
    fn entities(&self) -> Vec<E>;

    // Components
    /// check whether entity contains component
    fn has_component<R: ReplicateSafe<P>>(&self, entity: &E) -> bool;
    /// check whether entity contains component, dynamically
    fn has_component_of_kind(&self, entity: &E, component_kind: &P::Kind) -> bool;
    /// gets an entity's component
    fn get_component<'a, R: ReplicateSafe<P>>(
        &'a self,
        entity: &E,
    ) -> Option<ComponentRef<'a, P, R>>;
    /// gets an entity's component, dynamically
    fn get_component_of_kind(
        &self,
        entity: &E,
        component_kind: &P::Kind,
    ) -> Option<ComponentDynRef<'_, P>>;
}

/// Structures that implement the WorldMutType trait will be able to be loaded
/// into the Server at which point the Server will use this interface to keep
/// the WorldMutType in-sync with it's own Entities/Components
pub trait WorldMutType<P: ProtocolType, E: EntityType>:
    WorldRefType<P, E> + ProtocolInserter<P, E>
{
    // Entities
    /// spawn an entity
    fn spawn_entity(&mut self) -> E;
    /// despawn an entity
    fn despawn_entity(&mut self, entity: &E);

    // Components
    /// gets all of an Entity's Components as a list of Kinds
    fn get_component_kinds(&mut self, entity: &E) -> Vec<P::Kind>;
    /// gets an entity's component
    fn get_component_mut<'a, R: ReplicateSafe<P>>(
        &'a mut self,
        entity: &E,
    ) -> Option<ComponentMut<'a, P, R>>;
    /// gets a mutable component by type
    fn get_component_mut_of_kind(
        &mut self,
        entity: &E,
        component_kind: &P::Kind,
    ) -> Option<ComponentDynMut<'_, P>>;
    /// insert a component
    fn insert_component<R: ReplicateSafe<P>>(&mut self, entity: &E, component_ref: R);
    /// remove a component
    fn remove_component<R: Replicate<P>>(&mut self, entity: &E) -> Option<R>;
    /// remove a component by kind
    fn remove_component_of_kind(&mut self, entity: &E, component_kind: &P::Kind) -> Option<P>;
}
