use std::marker::PhantomData;

use bevy::{ecs::{entity::Entities, world::World, system::{SystemParam, SystemParamState, SystemParamFetch, SystemState}}, log::debug};

use naia_server::{ProtocolType, ImplRef, Replicate, Ref};

use crate::world::entity::Entity;

// Command Trait

pub trait Command: Send + Sync + 'static {
    fn write(self: Box<Self>, world: &mut World);
}

// CommandQueue

#[derive(Default)]
pub struct CommandQueue {
    commands: Vec<Box<dyn Command>>,
}

impl CommandQueue {
    pub fn apply(&mut self, world: &mut World) {
        // Have to do this to get around 'world.flush()' only being crate-public
        world.spawn().despawn();

        // Process queued commands
        for command in self.commands.drain(..) {
            command.write(world);
        }
    }

    #[inline]
    pub fn push_boxed(&mut self, command: Box<dyn Command>) {
        self.commands.push(command);
    }

    #[inline]
    pub fn push<T: Command>(&mut self, command: T) {
        self.push_boxed(Box::new(command));
    }
}

// SAFE: only local state is accessed
unsafe impl SystemParamState for CommandQueue {
    type Config = ();

    fn init(_world: &mut World, _system_state: &mut SystemState, _config: Self::Config) -> Self {
        Default::default()
    }

    fn apply(&mut self, world: &mut World) {
        self.apply(world);
    }

    fn default_config() {}
}

impl<'a> SystemParamFetch<'a> for CommandQueue {
    type Item = Commands<'a>;

    #[inline]
    unsafe fn get_param(
        state: &'a mut Self,
        _system_state: &'a SystemState,
        world: &'a World,
        _change_tick: u32,
    ) -> Self::Item {
        Commands::new(state, world)
    }
}

// Commands

pub struct Commands<'a> {
    queue: &'a mut CommandQueue,
    entities: &'a Entities,
}

impl<'a> Commands<'a> {
    pub fn new(queue: &'a mut CommandQueue, world: &'a World) -> Self {
        Self {
            queue,
            entities: world.entities(),
        }
    }

    pub fn spawn(&mut self) -> EntityCommands<'a, '_> {
        let entity = self.entities.reserve_entity();
        EntityCommands {
            entity: Entity::new(entity),
            commands: self,
        }
    }

    pub fn entity(&mut self, entity: &Entity) -> EntityCommands<'a, '_> {
        EntityCommands {
            entity: *entity,
            commands: self,
        }
    }

    pub(crate) fn add<C: Command>(&mut self, command: C) {
        self.queue.push(command);
    }
}

impl<'a> SystemParam for Commands<'a> {
    type Fetch = CommandQueue;
}

// EntityCommands

pub struct EntityCommands<'a, 'b> {
    entity: Entity,
    commands: &'b mut Commands<'a>,
}

impl<'a, 'b> EntityCommands<'a, 'b> {

    #[inline]
    pub fn id(&self) -> Entity {
        self.entity
    }

    pub fn insert<P: ProtocolType, R: ImplRef<P>>(&mut self, component_ref: &R) -> &mut Self {
        self.commands.add(Insert {
            entity: self.entity,
            component: component_ref.clone_ref(),
            phantom_p: PhantomData,
        });
        self
    }

    pub fn remove<P: ProtocolType, R: Replicate<P>>(&mut self) -> &mut Self
    {
        self.commands.add(Remove::<P, R> {
            entity: self.entity,
            phantom_p: PhantomData,
            phantom_r: PhantomData,
        });
        self
    }

    pub fn despawn(&mut self) {
        self.commands.add(Despawn {
            entity: self.entity,
        })
    }

    pub fn commands(&mut self) -> &mut Commands<'a> {
        self.commands
    }
}

// Specific Commands

//// despawn ////

#[derive(Debug)]
pub(crate) struct Despawn {
    entity: Entity,
}

impl Command for Despawn {
    fn write(self: Box<Self>, world: &mut World) {
        if !world.despawn(*self.entity) {
            debug!("Failed to despawn non-existent entity {:?}", self.entity);
        }
    }
}

//// insert ////

#[derive(Debug)]
pub(crate) struct Insert<P: ProtocolType, R: ImplRef<P>> {
    entity: Entity,
    component: R,
    phantom_p: PhantomData<P>,
}

impl<P: ProtocolType, R: ImplRef<P>> Command for Insert<P, R>
{
    fn write(self: Box<Self>, world: &mut World) {
        world.entity_mut(*self.entity).insert(self.component);
    }
}

//// remove ////

#[derive(Debug)]
pub(crate) struct Remove<P: ProtocolType, R: Replicate<P>> {
    entity: Entity,
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: ProtocolType, R: Replicate<P>> Command for Remove<P, R>
{
    fn write(self: Box<Self>, world: &mut World) {
        if let Some(mut entity_mut) = world.get_entity_mut(*self.entity) {
            entity_mut.remove::<Ref<R>>();
        }
    }
}