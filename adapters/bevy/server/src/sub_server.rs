// SubServer

use naia_bevy_shared::ComponentKind;

use crate::{world_entity::WorldEntity, Replicate};

pub(crate) struct SubServer {

}

impl Default for SubServer {
    fn default() -> Self {
        Self {}
    }
}

impl SubServer {
    pub(crate) fn enable_replication(&mut self, _world_entity: &WorldEntity) {
        todo!()
    }

    pub(crate) fn disable_replication(&mut self, _world_entity: &WorldEntity) {
        todo!()
    }

    pub(crate) fn pause_replication(&mut self, _world_entity: &WorldEntity) {
        todo!()
    }

    pub(crate) fn resume_replication(&mut self, _world_entity: &WorldEntity) {
        todo!()
    }

    pub(crate) fn despawn_entity_worldless(&mut self, _world_entity: &WorldEntity) {
        todo!()
    }

    pub(crate) fn insert_component_worldless(&mut self, _world_entity: &WorldEntity, _component: &mut dyn Replicate) {
        todo!()
    }

    pub(crate) fn remove_component_worldless(&mut self, _world_entity: &WorldEntity, _component_kind: &ComponentKind) {
        todo!()
    }
}