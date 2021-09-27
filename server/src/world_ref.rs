use naia_shared::{EntityKey, ProtocolType};

use super::{
    entity_ref::{EntityMut, EntityRef},
    server::Server,
    world_type::WorldType,
};

// WorldRef

pub struct WorldRef<'s, 'w, P: ProtocolType, W: WorldType> {
    server: &'s Server<P>,
    world: &'w W,
}

impl<'s, 'w, P: ProtocolType, W: WorldType> WorldRef<'s, 'w, P, W> {
    /// Return a new WorldRef
    pub fn new(server: &'s Server<P>, world: &'w W) -> Self {
        WorldRef { server, world }
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// Entity associated with the given EntityKey.
    /// Panics if the Entity does not exist.
    pub fn entity(&self, entity_key: &EntityKey) -> EntityRef<P, W> {
        if self.server.entity_exists(entity_key) {
            return EntityRef::new(self.server, self.world, &entity_key);
        }
        panic!("No Entity exists for given Key!");
    }
}

// WorldMut
pub struct WorldMut<'s, 'w, P: ProtocolType, W: WorldType> {
    server: &'s mut Server<P>,
    world: &'w mut W,
}

impl<'s, 'w, P: ProtocolType, W: WorldType> WorldMut<'s, 'w, P, W> {
    pub fn new(server: &'s mut Server<P>, world: &'w mut W) -> Self {
        WorldMut { server, world }
    }

    // Entities

    /// Spawns a new Entity and returns a corresponding EntityMut, which can be
    /// used for various operations
    pub fn spawn_entity(&mut self) -> EntityMut<P, W> {
        let entity_key: EntityKey = self.server.spawn_entity();

        self.world.spawn_entity(&entity_key);

        return EntityMut::new(self.server, self.world, &entity_key);
    }

    /// Retrieves an EntityMut that exposes read and write operations for the
    /// Entity associated with the given EntityKey.
    /// Panics if the entity does not exist.
    pub fn entity_mut(&mut self, entity_key: &EntityKey) -> EntityMut<P, W> {
        if self.server.entity_exists(entity_key) {
            return EntityMut::new(self.server, self.world, &entity_key);
        }
        panic!("No Entity exists for given Key!");
    }
}
