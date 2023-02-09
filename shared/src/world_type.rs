use naia_serde::SerdeErr;

use crate::{
    component::{
        component_kinds::ComponentKind,
        component_update::ComponentUpdate,
        replica_ref::{ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper},
        replicate::Replicate,
    },
    entity::entity_property::NetEntityHandleConverter,
};

/// Structures that implement the WorldMutType trait will be able to be loaded
/// into the Server at which point the Server will use this interface to keep
/// the WorldMutType in-sync with it's own Entities/Components
pub trait WorldRefType<E> {
    // Entities
    /// check whether entity exists
    fn has_entity(&self, entity: &E) -> bool;
    /// get a list of all entities in the World
    fn entities(&self) -> Vec<E>;

    // Components
    /// check whether entity contains component
    fn has_component<R: Replicate>(&self, entity: &E) -> bool;
    /// check whether entity contains component, dynamically
    fn has_component_of_kind(&self, entity: &E, component_kind: &ComponentKind) -> bool;
    /// gets an entity's component
    fn component<'a, R: Replicate>(&'a self, entity: &E) -> Option<ReplicaRefWrapper<'a, R>>;
    /// gets an entity's component, dynamically
    fn component_of_kind<'a>(
        &'a self,
        entity: &E,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>>;
}

/// Structures that implement the WorldMutType trait will be able to be loaded
/// into the Server at which point the Server will use this interface to keep
/// the WorldMutType in-sync with it's own Entities/Components
pub trait WorldMutType<E>: WorldRefType<E> {
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
    /// gets all of an Entity's Components
    fn component_kinds(&mut self, entity: &E) -> Vec<ComponentKind>;
    /// gets an entity's component
    fn component_mut<'a, R: Replicate>(
        &'a mut self,
        entity: &E,
    ) -> Option<ReplicaMutWrapper<'a, R>>;
    /// reads an incoming stream into a component
    fn component_apply_update(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        entity: &E,
        component_kind: &ComponentKind,
        update: ComponentUpdate,
    ) -> Result<(), SerdeErr>;
    /// mirrors the whole state of two different entities
    /// (setting 1st entity's component to 2nd entity's component's state)
    fn mirror_entities(&mut self, mutable_entity: &E, immutable_entity: &E);
    /// mirrors the state of the same component of two different entities
    /// (setting 1st entity's component to 2nd entity's component's state)
    fn mirror_components(
        &mut self,
        mutable_entity: &E,
        immutable_entity: &E,
        component_kind: &ComponentKind,
    );
    /// insert a component
    fn insert_component<R: Replicate>(&mut self, entity: &E, component_ref: R);
    /// insert a boxed component
    fn insert_boxed_component(&mut self, entity: &E, boxed_component: Box<dyn Replicate>);
    /// remove a component
    fn remove_component<R: Replicate>(&mut self, entity: &E) -> Option<R>;
    /// remove a component by kind
    fn remove_component_of_kind(
        &mut self,
        entity: &E,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>>;
}
