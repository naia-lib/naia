use bevy::ecs::prelude::Commands;

use naia_server::{Server, ImplRef, ProtocolType, EntityType, Replicate, UserKey};

use crate::world::entity::Entity;

// CommandsExt

pub trait CommandsExt<P: ProtocolType, K: EntityType> {
    fn with(&mut self, server: &mut Server<P, K>) -> ServerCommands;
}

impl<'a, P: ProtocolType, K: EntityType> CommandsExt<P, K> for Commands<'a> {
    fn with(&mut self, server: &mut Server<P, K>) -> ServerCommands {
        unimplemented!()
    }
}

// ServerCommands

enum ServerCommand {

}

pub struct ServerCommands<'a, 'c> {
    queue: Vec<ServerCommand>,
    bevy_commands: &'c mut Commands<'a>
}

impl<'a, 'c> ServerCommands<'a, 'c> {

    // Public Methods

    pub fn spawn<'x>(&'x mut self) -> EntityCommands<'a, 'c, 'x> {
        let entity = self.bevy_commands.spawn().id();
        let new_entity = Entity::new(entity);
        return EntityCommands::new(self, new_entity);
    }

    pub fn entity<'x>(&'x mut self, entity: &Entity) -> EntityCommands<'a, 'c, 'x> {
        let new_entity = *entity;
        return EntityCommands::new(self, new_entity);
    }

    // Crate-public Methods

    pub(crate) fn new(commands: &'c mut Commands<'a>) -> Self {
        ServerCommands {
            queue: Vec::new(),
            bevy_commands: commands,
        }
    }
}

// EntityCommands

pub struct EntityCommands<'a, 'c, 'x> {
    entity: Entity,
    server_commands: &'x mut ServerCommands<'a, 'c>,
}

impl<'a, 'c, 'x> EntityCommands<'a, 'c, 'x> {

    // Public Methods

    pub fn id(&self) -> Entity {
        return self.entity;
    }

    pub fn insert<P: ProtocolType, R: ImplRef<P>>(&mut self, component_ref: &R) -> Self {
        todo!()
    }

    pub fn remove<P: ProtocolType, R: Replicate<P>>(&mut self) -> Self {
        todo!()
    }

    pub fn set_owner(&mut self, user_key: &UserKey) -> Self {
        todo!()
    }

    pub fn despawn(&mut self) -> Self {
        todo!()
    }

    // Crate-public Methods
    pub(crate) fn new(server_commands: &'x mut ServerCommands<'a, 'c>, entity: Entity) -> Self {
        EntityCommands {
            server_commands,
            entity,
        }
    }
}
