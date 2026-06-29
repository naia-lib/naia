use std::collections::{HashMap, HashSet};

use bevy_app::App;
use bevy_ecs::{
    component::{Component, Mutable},
    entity::Entity,
    prelude::Resource,
    schedule::InternedScheduleLabel,
};

use naia_shared::{ComponentKind, Replicate};

use super::component_access::{ComponentAccess, ComponentAccessor};

#[derive(Resource, Default)]
pub struct WorldData {
    entities: HashSet<Entity>,
    kind_to_accessor_map: HashMap<ComponentKind, Box<dyn ComponentAccess>>,
    /// Component kinds that are Replicated Resources (registered via
    /// `Protocol::add_resource`). A ReplicatedResource is BOTH a bevy
    /// `Component` (the carrier entity-component) AND a bevy `Resource`
    /// (`Res<R>`); under bevy 0.19 those share one storage cell, so the
    /// carrier entity-component aliases `Res<R>`. Despawning such an
    /// entity panics — so the despawn chokepoint
    /// (`WorldMut::despawn_entity`) consults this set and emulates bevy's
    /// resource lifecycle (component-remove → `Res<R>` becomes `None`)
    /// instead of `World::despawn`.
    resource_kinds: HashSet<ComponentKind>,
    /// Entities that have ever carried a Replicated Resource component.
    /// Once an entity has held a `Resource`-derived component, bevy 0.19
    /// panics if it is `World::despawn`ed — *even after the component is
    /// removed*. Resource removal replicates as BOTH a component-remove
    /// (empties the carrier) and an entity-despawn; this set lets the
    /// despawn chokepoint recognize the now-empty carrier and skip
    /// `World::despawn` (the entity is retained, reclaimed on world drop).
    resource_carrier_entities: HashSet<Entity>,
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
            resource_kinds: self.resource_kinds.clone(),
            resource_carrier_entities: self.resource_carrier_entities.clone(),
        }
    }
}

impl WorldData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn merge(&mut self, other: Self) {
        if !self.entities.is_empty() || !other.entities.is_empty() {
            panic!("merging world data with non-empty entities");
        }
        self.kind_to_accessor_map.extend(other.kind_to_accessor_map);
        self.resource_kinds.extend(other.resource_kinds);
    }

    pub fn add_systems(&self, app: &mut App) {
        for accessor in self.kind_to_accessor_map.values() {
            accessor.add_systems(app);
        }
    }

    /// Like [`add_systems`], but registers every per-Replicate
    /// `on_component_added` / `on_component_removed` system in
    /// `schedule` instead of `Update`. Used by `Plugin::sim_integration`
    /// when the host bevy app's gameplay schedule is e.g. `SimMain`
    /// rather than the canonical `Update`.
    pub fn add_systems_to_schedule(&self, app: &mut App, schedule: InternedScheduleLabel) {
        for accessor in self.kind_to_accessor_map.values() {
            accessor.add_systems_to_schedule(app, schedule);
        }
    }

    // Entities //

    pub fn entities(&self) -> Vec<Entity> {
        let mut output = Vec::new();

        for entity in &self.entities {
            output.push(*entity);
        }

        output
    }

    pub fn spawn_entity(&mut self, entity: &Entity) {
        self.entities.insert(*entity);
    }

    pub fn despawn_entity(&mut self, entity: &Entity) {
        self.entities.remove(entity);
    }

    // Components

    #[allow(clippy::borrowed_box)]
    pub fn component_access(
        &self,
        component_kind: &ComponentKind,
    ) -> Option<&Box<dyn ComponentAccess>> {
        self.kind_to_accessor_map.get(component_kind)
    }

    /// D.3 sugar: lookup `ComponentAccess` by kind and invoke
    /// `insert_component` in one call. Panics if the kind is not registered.
    pub fn insert_component_dyn(
        &self,
        world: &mut bevy_ecs::world::World,
        entity: &Entity,
        kind: &ComponentKind,
        boxed: Box<dyn Replicate>,
    ) {
        let accessor = self
            .component_access(kind)
            .expect("ComponentKind must be registered in this WorldData");
        accessor.insert_component(world, entity, boxed);
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

    /// Mark `component_kind` as a Replicated Resource kind. Called by
    /// `Protocol::add_resource`.
    pub fn mark_resource_kind(&mut self, component_kind: &ComponentKind) {
        self.resource_kinds.insert(*component_kind);
    }

    /// Whether `component_kind` is a Replicated Resource kind (its
    /// carrier entity-component aliases `Res<R>` and must never be
    /// despawned — see [`WorldData::resource_kinds`]).
    pub fn is_resource_kind(&self, component_kind: &ComponentKind) -> bool {
        self.resource_kinds.contains(component_kind)
    }

    /// Record that `entity` carries (or has carried) a Replicated
    /// Resource component, so the despawn chokepoint never `World::despawn`s
    /// it.
    pub fn mark_resource_carrier_entity(&mut self, entity: &Entity) {
        self.resource_carrier_entities.insert(*entity);
    }

    /// Whether `entity` has ever carried a Replicated Resource component.
    pub fn is_resource_carrier_entity(&self, entity: &Entity) -> bool {
        self.resource_carrier_entities.contains(entity)
    }
}
