use naia_shared::{EntityKey, ImplRef, ProtocolType, Ref, Replicate};

use super::world_type::WorldType;

/// A default World which implements WorldType and that Naia can use to store
/// Entities/Components. It's recommended to use this only when you do not have
/// another ECS library's own World available.
pub struct World {}

impl WorldType for World {
    fn spawn_entity(&mut self, entity_key: &EntityKey) {
        unimplemented!()
    }

    fn despawn_entity(&mut self, entity_key: &EntityKey) {
        unimplemented!()
    }

    fn has_component<P: ProtocolType, R: Replicate<P>>(&self, entity_key: &EntityKey) -> bool {
        unimplemented!()
    }

    fn component<P: ProtocolType, R: Replicate<P>>(
        &self,
        entity_key: &EntityKey,
    ) -> Option<Ref<R>> {
        unimplemented!()
    }

    fn insert_component<P: ProtocolType, R: ImplRef<P>>(
        &mut self,
        entity_key: &EntityKey,
        component_ref: R,
    ) {
        unimplemented!()
    }

    fn remove_component<P: ProtocolType, R: Replicate<P>>(&mut self, entity_key: &EntityKey) {
        unimplemented!()
    }
}
