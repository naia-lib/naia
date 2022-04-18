use crate::{ComponentUpdate, Replicate};

use crate::protocol::{
    entity_property::NetEntityHandleConverter,
    protocolize::{ProtocolInserter, Protocolize},
    replica_ref::{ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper},
    replicate::ReplicateSafe,
};

/// Structures that implement the WorldMutType trait will be able to be loaded
/// into the Server at which point the Server will use this interface to keep
/// the WorldMutType in-sync with it's own Entities/Components
pub trait WorldRefType<P: Protocolize, E> {
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
    fn component<'a, R: ReplicateSafe<P>>(
        &'a self,
        entity: &E,
    ) -> Option<ReplicaRefWrapper<'a, P, R>>;
    /// gets an entity's component, dynamically
    fn component_of_kind<'a>(
        &'a self,
        entity: &E,
        component_kind: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<'a, P>>;
}

/// Structures that implement the WorldMutType trait will be able to be loaded
/// into the Server at which point the Server will use this interface to keep
/// the WorldMutType in-sync with it's own Entities/Components
pub trait WorldMutType<P: Protocolize, E>: WorldRefType<P, E> + ProtocolInserter<P, E> {
    // Entities
    /// spawn an entity
    fn spawn_entity(&mut self) -> E;
    /// duplicate an entity
    fn duplicate_entity(&mut self, entity: &E) -> E;
    /// make it so one entity has all the same components as another
    fn duplicate_components(&mut self, mutable_entity: &E, immutable_entity: &E);
    /// despawn an entity
    fn despawn_entity(&mut self, entity: &E);

    // Components
    /// gets all of an Entity's Components as a list of Kinds
    fn component_kinds(&mut self, entity: &E) -> Vec<P::Kind>;
    /// gets an entity's component
    fn component_mut<'a, R: ReplicateSafe<P>>(
        &'a mut self,
        entity: &E,
    ) -> Option<ReplicaMutWrapper<'a, P, R>>;
    /// reads an incoming stream into a component
    fn component_apply_update(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        entity: &E,
        component_kind: &P::Kind,
        update: ComponentUpdate<P::Kind>,
    );
    /// mirrors the whole state of two different entities
    /// (setting 1st entity's component to 2nd entity's component's state)
    fn mirror_entities(&mut self, mutable_entity: &E, immutable_entity: &E);
    /// mirrors the state of the same component of two different entities
    /// (setting 1st entity's component to 2nd entity's component's state)
    fn mirror_components(
        &mut self,
        mutable_entity: &E,
        immutable_entity: &E,
        component_kind: &P::Kind,
    );
    /// insert a component
    fn insert_component<R: ReplicateSafe<P>>(&mut self, entity: &E, component_ref: R);
    /// remove a component
    fn remove_component<R: Replicate<P>>(&mut self, entity: &E) -> Option<R>;
    /// remove a component by kind
    fn remove_component_of_kind(&mut self, entity: &E, component_kind: &P::Kind) -> Option<P>;
}
