use std::any::{Any, TypeId};
use std::collections::HashMap;

use bevy_ecs::{entity::Entity, component::Component, system::Resource};

#[derive(Component, Clone, Copy)]
pub struct HostOwned {
    type_id: TypeId
}

impl HostOwned {
    pub fn new<T: Any>() -> Self {
        Self {
            type_id: TypeId::of::<T>()
        }
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }
}

#[derive(Resource)]
pub struct HostOwnedMap {
    map: HashMap<Entity, HostOwned>,
}

impl Default for HostOwnedMap {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

impl HostOwnedMap {
    pub fn insert(&mut self, entity: Entity, host_owned: HostOwned) {
        self.map.insert(entity, host_owned);
    }

    pub fn remove(&mut self, entity: &Entity) -> Option<HostOwned> {
        self.map.remove(entity)
    }
}