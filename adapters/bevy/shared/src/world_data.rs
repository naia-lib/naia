use std::{
    collections::{HashMap, HashSet},
    default::Default,
};

use bevy_app::App;
use bevy_ecs::{
    component::{Component, Mutable},
    entity::Entity,
    prelude::Resource,
    world::{FromWorld, World},
};

use naia_shared::{ComponentKind, Replicate};

use super::component_access::{ComponentAccess, ComponentAccessor};

#[derive(Resource)]
pub struct WorldData {
    entities: HashSet<Entity>,
    kind_to_accessor_map: HashMap<ComponentKind, Box<dyn ComponentAccess>>,
}

impl Clone for WorldData {
    fn clone(&self) -> Self {
        Self {
            entities: self.entities.clone(),
            kind_to_accessor_map: self
                .kind_to_accessor_map
                .iter()
                .map(|(kind, accessor)| (*kind, accessor.box_clone()))
                .collect(),
        }
    }
}

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

    pub fn merge(&mut self, other: Self) {
        if !self.entities.is_empty() || !other.entities.is_empty() {
            panic!("merging world data with non-empty entities");
        }
        self.kind_to_accessor_map.extend(other.kind_to_accessor_map);
    }

    pub fn add_systems(&self, app: &mut App) {
        for (_kind, accessor) in &self.kind_to_accessor_map {
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
        self.kind_to_accessor_map.get(component_kind)
    }

    pub(crate) fn put_kind<R: Replicate + Component<Mutability = Mutable>>(
        &mut self,
        component_kind: &ComponentKind,
    ) {
        self.kind_to_accessor_map
            .insert(*component_kind, ComponentAccessor::<R>::create());
    }

    pub(crate) fn has_kind(&self, component_kind: &ComponentKind) -> bool {
        self.kind_to_accessor_map.contains_key(component_kind)
    }
}
