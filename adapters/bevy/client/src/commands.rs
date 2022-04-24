use std::marker::PhantomData;

use bevy_ecs::{
    entity::Entity,
    system::{Command, Commands, EntityCommands},
    world::World,
};

use naia_bevy_shared::WorldProxyMut;

use naia_client::shared::{Protocolize, WorldMutType};

pub trait CommandsExt<'w, 's, P: Protocolize> {
    fn duplicate_entity<'a>(&'a mut self, entity: Entity) -> EntityCommands<'w, 's, 'a>;
    fn mirror_entities(&mut self, mutable_entity: Entity, immutable_entity: Entity);
}

impl<'w, 's, P: Protocolize> CommandsExt<'w, 's, P> for Commands<'w, 's> {
    fn duplicate_entity<'a>(&'a mut self, entity: Entity) -> EntityCommands<'w, 's, 'a> {
        let new_entity = self.spawn().id();
        let command = DuplicateComponents::<P>::new(new_entity, entity);
        self.add(command);
        self.entity(new_entity)
    }

    fn mirror_entities(&mut self, mutable_entity: Entity, immutable_entity: Entity) {
        self.add(MirrorEntities::<P>::new(mutable_entity, immutable_entity));
    }
}

//// DuplicateComponents Command ////

pub(crate) struct DuplicateComponents<P: Protocolize> {
    mutable_entity: Entity,
    immutable_entity: Entity,
    phantom_p: PhantomData<P>,
}

impl<P: Protocolize> DuplicateComponents<P> {
    pub fn new(new_entity: Entity, old_entity: Entity) -> Self {
        Self {
            mutable_entity: new_entity,
            immutable_entity: old_entity,
            phantom_p: PhantomData,
        }
    }
}

impl<P: Protocolize> Command for DuplicateComponents<P> {
    fn write(self, world: &mut World) {
        WorldMutType::<P, Entity>::duplicate_components(
            &mut world.proxy_mut(),
            &self.mutable_entity,
            &self.immutable_entity,
        );
    }
}

//// MirrorEntities Command ////

pub(crate) struct MirrorEntities<P: Protocolize> {
    mutable_entity: Entity,
    immutable_entity: Entity,
    phantom_p: PhantomData<P>,
}

impl<P: Protocolize> MirrorEntities<P> {
    pub fn new(new_entity: Entity, old_entity: Entity) -> Self {
        Self {
            mutable_entity: new_entity,
            immutable_entity: old_entity,
            phantom_p: PhantomData,
        }
    }
}

impl<P: Protocolize> Command for MirrorEntities<P> {
    fn write(self, world: &mut World) {
        WorldMutType::<P, Entity>::mirror_entities(
            &mut world.proxy_mut(),
            &self.mutable_entity,
            &self.immutable_entity,
        );
    }
}
