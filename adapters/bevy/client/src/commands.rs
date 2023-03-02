use bevy_ecs::{
    entity::Entity,
    system::{Command as BevyCommand, Commands, EntityCommands},
    world::World,
};
use std::marker::PhantomData;

use naia_client::Client;

use naia_bevy_shared::{Replicate, WorldMut, WorldMutType, WorldProxyMut};

// Naia Client Command Trait
pub trait Command: Send + Sync + 'static {
    fn write(self: Box<Self>, server: &mut Client<Entity>, world: WorldMut);
}

//// Despawn Entity ////

pub(crate) struct DespawnEntity {
    entity: Entity,
}

impl DespawnEntity {
    pub fn new(entity: &Entity) -> Self {
        DespawnEntity { entity: *entity }
    }
}

impl Command for DespawnEntity {
    fn write(self: Box<Self>, client: &mut Client<Entity>, world: WorldMut) {
        client.entity_mut(world, &self.entity).despawn();
    }
}

//// Insert Component ////

pub(crate) struct InsertComponent<R: Replicate> {
    entity: Entity,
    component: R,
}

impl<R: Replicate> InsertComponent<R> {
    pub fn new(entity: &Entity, component: R) -> Self {
        InsertComponent {
            entity: *entity,
            component,
        }
    }
}

impl<R: Replicate> Command for InsertComponent<R> {
    fn write(self: Box<Self>, client: &mut Client<Entity>, world: WorldMut) {
        client
            .entity_mut(world, &self.entity)
            .insert_component(self.component);
    }
}

//// Remove Component ////

pub(crate) struct RemoveComponent<R: Replicate> {
    entity: Entity,
    phantom_r: PhantomData<R>,
}

impl<R: Replicate> RemoveComponent<R> {
    pub fn new(entity: &Entity) -> Self {
        RemoveComponent {
            entity: *entity,
            phantom_r: PhantomData,
        }
    }
}

impl<R: Replicate> Command for RemoveComponent<R> {
    fn write(self: Box<Self>, client: &mut Client<Entity>, world: WorldMut) {
        client
            .entity_mut(world, &self.entity)
            .remove_component::<R>();
    }
}

// Bevy Commands Extension
pub trait CommandsExt<'w, 's> {
    fn duplicate_entity<'a>(&'a mut self, entity: Entity) -> EntityCommands<'w, 's, 'a>;
}

impl<'w, 's> CommandsExt<'w, 's> for Commands<'w, 's> {
    fn duplicate_entity<'a>(&'a mut self, entity: Entity) -> EntityCommands<'w, 's, 'a> {
        let new_entity = self.spawn_empty().id();
        let command = DuplicateComponents::new(new_entity, entity);
        self.add(command);
        self.entity(new_entity)
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

impl BevyCommand for DuplicateComponents {
    fn write(self, world: &mut World) {
        WorldMutType::<Entity>::duplicate_components(
            &mut world.proxy_mut(),
            &self.mutable_entity,
            &self.immutable_entity,
        );
    }
}
