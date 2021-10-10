use bevy::ecs::prelude::Commands;

use naia_server::{Server, ImplRef, ProtocolType, EntityType, Replicate, UserKey};

use crate::world::entity::Entity;

// CommandsExt

pub trait CommandsExt<'a, 'c, 's, P: ProtocolType> {
    fn with(&'c mut self, server: &'s mut Server<P, Entity>) -> ServerCommands<'a, 'c, 's, P>;
}

impl<'a, 'c, 's, P: ProtocolType> CommandsExt<'a, 'c, 's, P> for Commands<'a> {
    fn with(&'c mut self, server: &'s mut Server<P, Entity>) -> ServerCommands<'a, 'c, 's, P> {
        return ServerCommands::new(self, server);
    }
}

// ServerCommand

enum ServerCommand<P: ProtocolType> {
    InsertComponent(Entity, P)
}

// ServerCommands

pub struct ServerCommands<'a, 'c, 's, P: ProtocolType> {
    queue: Vec<ServerCommand<P>>,
    bevy_commands: &'c mut Commands<'a>,
    naia_server: &'s mut Server<P, Entity>,
}

impl<'a, 'c, 's, P: ProtocolType> ServerCommands<'a, 'c, 's, P> {

    // Public Methods

    pub fn spawn<'x>(&'x mut self) -> EntityCommands<'a, 'c, 's, 'x, P>{
        let entity = self.bevy_commands.spawn().id();
        let new_entity = Entity::new(entity);
        return EntityCommands::new(self, new_entity);
    }

    pub fn entity<'x>(&'x mut self, entity: &Entity) -> EntityCommands<'a, 'c, 's, 'x, P> {
        let new_entity = *entity;
        return EntityCommands::new(self, new_entity);
    }

    // Crate-public Methods

    pub(crate) fn new(commands: &'c mut Commands<'a>, server: &'s mut Server<P, Entity>) -> Self {
        ServerCommands {
            queue: Vec::new(),
            bevy_commands: commands,
            naia_server: server,
        }
    }

    pub(crate) fn insert<R: ImplRef<P>>(&mut self, entity: &Entity, component_ref: &R) {
        // bevy
        self.bevy_commands.entity(**entity).insert(component_ref.clone_ref());

        // naia
        self.queue.push(ServerCommand::InsertComponent(*entity, component_ref.protocol()));
    }
}

// EntityCommands

pub struct EntityCommands<'a, 'c, 's, 'x, P: ProtocolType> {
    entity: Entity,
    server_commands: &'x mut ServerCommands<'a, 'c, 's, P>,
}

impl<'a, 'c, 's, 'x, P: ProtocolType> EntityCommands<'a, 'c, 's, 'x, P> {

    // Public Methods

    pub fn id(&self) -> Entity {
        return self.entity;
    }

    pub fn insert<R: ImplRef<P>>(&mut self, component_ref: &R) -> &mut Self {
        self.server_commands.insert(&self.entity, component_ref);
        self
    }

    pub fn remove<R: Replicate<P>>(&mut self) -> Self {
        todo!()
    }

    pub fn set_owner(&mut self, user_key: &UserKey) -> Self {
        todo!()
    }

    pub fn despawn(&mut self) -> Self {
        todo!()
    }

    // Crate-public Methods
    pub(crate) fn new(server_commands: &'x mut ServerCommands<'a, 'c, 's, P>, entity: Entity) -> Self {
        EntityCommands {
            server_commands,
            entity,
        }
    }
}
