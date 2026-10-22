use std::collections::{HashMap, HashSet};

use bevy_app::{App, Update};
use bevy_ecs::{
    component::{Component, Mutable},
    entity::Entity,
    prelude::Resource,
    schedule::{InternedScheduleLabel, IntoScheduleConfigs, ScheduleLabel},
};

use naia_shared::{ComponentKind, Replicate};

use super::{
    component_access::{ComponentAccess, ComponentAccessor},
    system_set::{HostSyncDespawnTrailer, HostSyncKindSlot},
};

/// Per-`Protocol` bevy-world bookkeeping: tracks spawned entities, the
/// `ComponentAccess` for each registered component kind, and which kinds
/// and entities are Replicated Resources (so the despawn chokepoint can
/// emulate bevy's resource lifecycle instead of calling `World::despawn`).
#[derive(Resource, Default)]
pub struct WorldData {
    entities: HashSet<Entity>,
    kind_to_accessor_map: HashMap<ComponentKind, Box<dyn ComponentAccess>>,
    /// Component kinds in protocol registration order (the `ComponentKinds`
    /// net-ID order). `put_kind` is called once per `register_component` in
    /// registration order, so this mirrors the wire order both ends agree
    /// on. Tracker registration and kind enumeration walk this order so
    /// `HashMap` iteration order never leaks into system execution.
    registration_order: Vec<ComponentKind>,
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
            registration_order: self.registration_order.clone(),
            resource_kinds: self.resource_kinds.clone(),
            resource_carrier_entities: self.resource_carrier_entities.clone(),
        }
    }
}

impl WorldData {
    /// Creates an empty `WorldData`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merges `other`'s registered component kinds and resource kinds into
    /// `self`. Panics if either side already has spawned entities.
    pub fn merge(&mut self, other: Self) {
        if !self.entities.is_empty() || !other.entities.is_empty() {
            panic!("merging world data with non-empty entities");
        }
        for kind in &other.registration_order {
            if !self.kind_to_accessor_map.contains_key(kind) {
                self.registration_order.push(*kind);
            }
        }
        self.kind_to_accessor_map.extend(other.kind_to_accessor_map);
        self.resource_kinds.extend(other.resource_kinds);
    }

    /// Registers every component kind's `on_component_added` /
    /// `on_component_removed` systems under `Update` on `app`, in protocol
    /// registration order: each kind's added-before-removed pair joins its
    /// own [`HostSyncKindSlot`], and consecutive slots (plus the trailing
    /// [`HostSyncDespawnTrailer`], where `on_despawn` is registered by the
    /// plugins) are chained so the trackers execute as one deterministic
    /// sequence.
    pub fn add_systems(&self, app: &mut App) {
        for (ordinal, kind) in self.registration_order.iter().enumerate() {
            let accessor = self
                .kind_to_accessor_map
                .get(kind)
                .expect("every registered ComponentKind must have an accessor");
            accessor.add_systems_in_slot(app, HostSyncKindSlot(ordinal as u32));
        }
        self.configure_tracker_ordering(app, Update);
    }

    /// Like [`WorldData::add_systems`], but registers every per-Replicate
    /// `on_component_added` / `on_component_removed` system in
    /// `schedule` instead of `Update`. Used by `Plugin::sim_integration`
    /// when the host bevy app's gameplay schedule is e.g. `SimMain`
    /// rather than the canonical `Update`.
    pub fn add_systems_to_schedule(&self, app: &mut App, schedule: InternedScheduleLabel) {
        for (ordinal, kind) in self.registration_order.iter().enumerate() {
            let accessor = self
                .kind_to_accessor_map
                .get(kind)
                .expect("every registered ComponentKind must have an accessor");
            accessor.add_systems_to_schedule_in_slot(
                app,
                schedule,
                HostSyncKindSlot(ordinal as u32),
            );
        }
        self.configure_tracker_ordering(app, schedule);
    }

    /// Chains the per-kind [`HostSyncKindSlot`]s in registration order and
    /// the [`HostSyncDespawnTrailer`] behind the last one, inside
    /// `schedule`. With no registered kinds there is nothing to chain;
    /// `on_despawn` still joins the trailer slot on its own.
    fn configure_tracker_ordering<S: ScheduleLabel + Clone>(&self, app: &mut App, schedule: S) {
        let count = self.registration_order.len() as u32;
        for ordinal in 0..count.saturating_sub(1) {
            app.configure_sets(
                schedule.clone(),
                HostSyncKindSlot(ordinal).before(HostSyncKindSlot(ordinal + 1)),
            );
        }
        if count > 0 {
            app.configure_sets(
                schedule,
                HostSyncKindSlot(count - 1).before(HostSyncDespawnTrailer),
            );
        }
    }

    // Entities //

    /// Returns every entity currently tracked as spawned.
    pub fn entities(&self) -> Vec<Entity> {
        let mut output = Vec::new();

        for entity in &self.entities {
            output.push(*entity);
        }

        output
    }

    /// Records `entity` as spawned.
    pub fn spawn_entity(&mut self, entity: &Entity) {
        self.entities.insert(*entity);
    }

    /// Stops tracking `entity` as spawned.
    pub fn despawn_entity(&mut self, entity: &Entity) {
        self.entities.remove(entity);
    }

    // Components

    /// Every registered replicated-component kind, in protocol registration
    /// order. Used by the host-sync initial-insert pass, which enumerates
    /// an entity's current components when tracking begins (cross-frame
    /// enable); registration order keeps the emitted `Insert` events
    /// deterministic across processes.
    pub fn component_kinds(&self) -> impl Iterator<Item = &ComponentKind> {
        self.registration_order.iter()
    }

    /// Returns the registered `ComponentAccess` for `component_kind`, if any.
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
        // `register_component` calls this once per kind in registration
        // order, so pushing unseen kinds here mirrors the protocol (wire
        // net-ID) order both ends agree on.
        if !self.kind_to_accessor_map.contains_key(component_kind) {
            self.registration_order.push(*component_kind);
        }
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
    /// despawned — see `WorldData::resource_kinds`).
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
