use std::{
    any::Any,
    collections::{HashMap, HashSet},
    default::Default,
};

use bevy_app::App;
use bevy_ecs::{
    entity::Entity,
    prelude::Resource,
    world::{FromWorld, World},
};

use naia_shared::{ComponentKind, Replicate};

use super::component_access::{ComponentAccess, ComponentAccessor};

#[derive(Resource)]
pub struct WorldData {
    entities: HashSet<Entity>,
    kind_to_accessor_map: HashMap<ComponentKind, Box<dyn Any>>,
}

unsafe impl Send for WorldData {}
unsafe impl Sync for WorldData {}

impl FromWorld for WorldData {
    fn from_world(_world: &mut World) -> Self {
        Self {
            entities: HashSet::default(),
            kind_to_accessor_map: HashMap::default(),
        }
    }
}

impl WorldData {
    pub fn new() -> Self {
        Self {
            entities: HashSet::default(),
            kind_to_accessor_map: HashMap::default(),
        }
    }

    pub fn add_systems(&self, app: &mut App) {
        for (_kind, accessor_any) in &self.kind_to_accessor_map {
            let accessor = accessor_any
                .downcast_ref::<Box<dyn ComponentAccess>>()
                .unwrap();
            accessor.add_systems(app);
        }
    }

    // Entities //

    pub(crate) fn entities(&self) -> Vec<Entity> {
        let mut output = Vec::new();

        for entity in &self.entities {
            output.push(*entity);
        }

        output
    }

    pub(crate) fn spawn_entity(&mut self, entity: &Entity) {
        self.entities.insert(*entity);
    }

    pub(crate) fn despawn_entity(&mut self, entity: &Entity) {
        self.entities.remove(entity);
    }

    // Components

    #[allow(clippy::borrowed_box)]
    pub(crate) fn component_access(
        &self,
        component_kind: &ComponentKind,
    ) -> Option<&Box<dyn ComponentAccess>> {
        if let Some(accessor_any) = self.kind_to_accessor_map.get(component_kind) {
            return accessor_any.downcast_ref::<Box<dyn ComponentAccess>>();
        }
        None
    }

    pub(crate) fn put_kind<R: Replicate>(&mut self, component_kind: &ComponentKind) {
        self.kind_to_accessor_map
            .insert(*component_kind, ComponentAccessor::<R>::create());
    }

    pub(crate) fn has_kind(&self, component_kind: &ComponentKind) -> bool {
        self.kind_to_accessor_map.contains_key(component_kind)
    }
}
