use naia_server::{ImplRef, ProtocolType, Replicate, UserKey};

use crate::world::entity::Entity;

pub struct ServerCommands {}

impl ServerCommands {
    pub fn new() -> Self {
        ServerCommands {}
    }

    pub fn spawn(&mut self) -> EntityCommands {
        todo!()
    }

    pub fn entity(&mut self, entity: &Entity) -> EntityCommands {
        todo!()
    }
}

pub struct EntityCommands {}

impl EntityCommands {
    pub fn new() -> Self {
        EntityCommands {}
    }

    pub fn id(&self) -> Entity {
        todo!()
    }

    pub fn insert<P: ProtocolType, R: ImplRef<P>>(&mut self, component_ref: &R) -> EntityCommands {
        todo!()
    }

    pub fn remove<P: ProtocolType, R: Replicate<P>>(&mut self) -> EntityCommands {
        todo!()
    }

    pub fn set_owner(&mut self, user_key: &UserKey) -> EntityCommands {
        todo!()
    }

    pub fn despawn(&mut self) -> EntityCommands {
        todo!()
    }
}
