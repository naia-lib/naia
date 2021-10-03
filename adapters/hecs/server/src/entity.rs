use std::ops::Deref;

use hecs::Entity as HecsEntity;

use naia_server::EntityType;

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Entity(HecsEntity);

impl Entity {
    pub fn new(entity: HecsEntity) -> Self {
        return Entity(entity);
    }
}

impl EntityType for Entity {}

impl Deref for Entity {
    type Target = HecsEntity;

    fn deref(&self) -> &Self::Target {
        return &self.0;
    }
}
