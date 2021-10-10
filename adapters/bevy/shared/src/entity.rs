use std::ops::Deref;

use bevy::ecs::entity::Entity as BevyEntity;

use naia_shared::EntityType;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entity(BevyEntity);

impl Entity {
    pub fn new(entity: BevyEntity) -> Self {
        return Entity(entity);
    }
}

impl EntityType for Entity {}

impl Deref for Entity {
    type Target = BevyEntity;

    fn deref(&self) -> &Self::Target {
        return &self.0;
    }
}
