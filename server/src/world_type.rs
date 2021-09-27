use naia_shared::{ImplRef, ProtocolType, Ref, Replicate};

use super::keys::entity_key::EntityKey;

/// Structures that implement the WorldType trait will be able to be loaded into
/// the Server at which point the Server will use this interface to keep the
/// WorldType in-sync with it's own Entities/Components
pub trait WorldType {
    /// spawn an entity
    fn spawn_entity(&mut self, entity_key: &EntityKey);
    /// despawn an entity
    fn despawn_entity(&mut self, entity_key: &EntityKey);
    /// check whether entity contains component
    fn has_component<P: ProtocolType, R: Replicate<P>>(&self, entity_key: &EntityKey) -> bool;
    /// gets an entity's component
    fn component<P: ProtocolType, R: Replicate<P>>(&self, entity_key: &EntityKey)
        -> Option<Ref<R>>;
    /// insert a component
    fn insert_component<P: ProtocolType, R: ImplRef<P>>(
        &mut self,
        entity_key: &EntityKey,
        component_ref: R,
    );
    /// remove a component
    fn remove_component<P: ProtocolType, R: Replicate<P>>(&mut self, entity_key: &EntityKey);
}
