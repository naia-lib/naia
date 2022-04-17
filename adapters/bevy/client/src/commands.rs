use std::marker::PhantomData;

use bevy::ecs::entity::Entity;

use naia_client::{
    shared::{ChannelIndex, Protocolize, Replicate},
    Client,
};

use naia_bevy_shared::WorldMut;

// Command Trait

pub trait Command<P: Protocolize, C: ChannelIndex>: Send + Sync + 'static {
    fn write(self: Box<Self>, server: &mut Client<P, Entity, C>, world: WorldMut);
}

//// Insert Component ////

#[derive(Debug)]
pub(crate) struct DuplicateEntity {
    entity: Entity,
}

impl DuplicateEntity {
    pub fn new(entity: &Entity) -> Self {
        return Self {
            entity: *entity,
        };
    }
}

impl<P: Protocolize, C: ChannelIndex> Command<P, C> for DuplicateEntity {
    fn write(self: Box<Self>, client: &mut Client<P, Entity, C>, world: WorldMut) {
        client.duplicate_entity(world, &self.entity);
    }
}