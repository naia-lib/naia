use bevy_ecs::entity::Entity;

use naia_server::{UserScopeRef as InnerUserScopeRef, UserScopeMut as InnerUserScopeMut};

use crate::{WorldId, world_entity::WorldEntity};

//// UserScopeRef ////

pub struct UserScopeRef<'a> {
    inner: InnerUserScopeRef<'a, WorldEntity>,
    world_id: WorldId,
}

impl<'a> UserScopeRef<'a> {
    pub(crate) fn new(world_id: WorldId, inner: InnerUserScopeRef<'a, WorldEntity>) -> Self {
        Self { world_id, inner }
    }

    pub fn has(&self, entity: &Entity) -> bool {
        let world_entity = WorldEntity::new(self.world_id, *entity);
        self.inner.has(&world_entity)
    }
}

//// UserScopeMut ////

pub struct UserScopeMut<'a> {
    inner: InnerUserScopeMut<'a, WorldEntity>,
    world_id: WorldId,
}

impl<'a> UserScopeMut<'a> {
    pub(crate) fn new(world_id: WorldId, inner: InnerUserScopeMut<'a, WorldEntity>) -> Self {
        Self { world_id, inner }
    }

    pub fn has(&self, entity: &Entity) -> bool {
        let world_entity = WorldEntity::new(self.world_id, *entity);
        self.inner.has(&world_entity)
    }

    pub fn include(&mut self, entity: &Entity) -> &mut Self {
        let world_entity = WorldEntity::new(self.world_id, *entity);
        self.inner.include(&world_entity);

        self
    }

    pub fn exclude(&mut self, entity: &Entity) -> &mut Self {
        let world_entity = WorldEntity::new(self.world_id, *entity);
        self.inner.exclude(&world_entity);

        self
    }

    pub fn clear(&mut self) -> &mut Self {
        self.inner.clear();

        self
    }
}