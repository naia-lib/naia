use bevy_ecs::{
    entity::Entity,
    system::{Command, Commands, EntityCommands},
    world::World,
};

use naia_bevy_shared::{WorldMutType, WorldProxyMut};

pub trait CommandsExt<'w, 's> {
    fn duplicate_entity<'a>(&'a mut self, entity: Entity) -> EntityCommands<'w, 's, 'a>;
    fn mirror_entities(&mut self, mutable_entity: Entity, immutable_entity: Entity);
}

impl<'w, 's> CommandsExt<'w, 's> for Commands<'w, 's> {
    fn duplicate_entity<'a>(&'a mut self, entity: Entity) -> EntityCommands<'w, 's, 'a> {
        let new_entity = self.spawn_empty().id();
        let command = DuplicateComponents::new(new_entity, entity);
        self.add(command);
        self.entity(new_entity)
    }

    fn mirror_entities(&mut self, mutable_entity: Entity, immutable_entity: Entity) {
        self.add(MirrorEntities::new(mutable_entity, immutable_entity));
    }
}

//// DuplicateComponents Command ////

pub(crate) struct DuplicateComponents {
    mutable_entity: Entity,
    immutable_entity: Entity,
}

impl DuplicateComponents {
    pub fn new(new_entity: Entity, old_entity: Entity) -> Self {
        Self {
            mutable_entity: new_entity,
            immutable_entity: old_entity,
        }
    }
}

impl Command for DuplicateComponents {
    fn write(self, world: &mut World) {
        WorldMutType::<Entity>::duplicate_components(
            &mut world.proxy_mut(),
            &self.mutable_entity,
            &self.immutable_entity,
        );
    }
}

//// MirrorEntities Command ////

pub(crate) struct MirrorEntities {
    mutable_entity: Entity,
    immutable_entity: Entity,
}

impl MirrorEntities {
    pub fn new(new_entity: Entity, old_entity: Entity) -> Self {
        Self {
            mutable_entity: new_entity,
            immutable_entity: old_entity,
        }
    }
}

impl Command for MirrorEntities {
    fn write(self, world: &mut World) {
        WorldMutType::<Entity>::mirror_entities(
            &mut world.proxy_mut(),
            &self.mutable_entity,
            &self.immutable_entity,
        );
    }
}
