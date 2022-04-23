use std::{hash::Hash, marker::PhantomData};

use naia_shared::{
    ChannelIndex, Protocolize, ReplicaMutWrapper, ReplicaRefWrapper, Replicate, ReplicateSafe,
    WorldMutType, WorldRefType,
};

use crate::{room::RoomKey, server::Server};

// EntityRef

/// A reference to an Entity being tracked by the Server
pub struct EntityRef<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>> {
    phantom_p: PhantomData<P>,
    world: W,
    entity: E,
}

impl<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>> EntityRef<P, E, W> {
    /// Return a new EntityRef
    pub(crate) fn new(world: W, entity: &E) -> Self {
        EntityRef {
            phantom_p: PhantomData,
            world,
            entity: *entity,
        }
    }

    /// Return the Entity itself
    pub fn id(&self) -> E {
        self.entity
    }

    // Components

    /// Returns whether or not the Entity has an associated Component
    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    /// Gets a Ref to a Component associated with the Entity
    pub fn component<R: ReplicateSafe<P>>(&self) -> Option<ReplicaRefWrapper<P, R>> {
        self.world.component::<R>(&self.entity)
    }
}

// EntityMut
pub struct EntityMut<
    's,
    P: Protocolize,
    E: Copy + Eq + Hash + Send + Sync,
    W: WorldMutType<P, E>,
    C: ChannelIndex,
> {
    server: &'s mut Server<P, E, C>,
    world: W,
    entity: E,
}

impl<
        's,
        P: Protocolize,
        E: Copy + Eq + Hash + Send + Sync,
        W: WorldMutType<P, E>,
        C: ChannelIndex,
    > EntityMut<'s, P, E, W, C>
{
    pub(crate) fn new(server: &'s mut Server<P, E, C>, world: W, entity: &E) -> Self {
        EntityMut {
            server,
            world,
            entity: *entity,
        }
    }

    pub fn id(&self) -> E {
        self.entity
    }

    pub fn despawn(&mut self) {
        self.server.despawn_entity(&mut self.world, &self.entity);
    }

    // Components

    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    pub fn component<R: ReplicateSafe<P>>(&mut self) -> Option<ReplicaMutWrapper<P, R>> {
        self.world.component_mut::<R>(&self.entity)
    }

    pub fn insert_component<R: ReplicateSafe<P>>(&mut self, component_ref: R) -> &mut Self {
        self.server
            .insert_component(&mut self.world, &self.entity, component_ref);

        self
    }

    pub fn insert_components<R: ReplicateSafe<P>>(
        &mut self,
        mut component_refs: Vec<R>,
    ) -> &mut Self {
        while let Some(component_ref) = component_refs.pop() {
            self.insert_component(component_ref);
        }

        self
    }

    pub fn remove_component<R: Replicate<P>>(&mut self) -> Option<R> {
        self.server
            .remove_component::<R, W>(&mut self.world, &self.entity)
    }

    // Rooms

    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_add_entity(room_key, &self.entity);

        self
    }

    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_remove_entity(room_key, &self.entity);

        self
    }
}
