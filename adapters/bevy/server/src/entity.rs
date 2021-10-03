use std::ops::Deref;

use bevy::ecs::{
    entity::Entity as BevyEntity,
};

use naia_server::KeyType;

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Entity(BevyEntity);

impl Entity {
    pub fn new(entity: BevyEntity) -> Self {
        return Entity(entity);
    }
}

impl KeyType for Entity {}

impl Deref for Entity {
    type Target = BevyEntity;
    fn deref(&self) -> &Self::Target {
        return &self.0;
    }
}