use std::{any::TypeId, marker::PhantomData};

use bevy_ecs::{
    entity::Entity,
    world::{Mut, World},
};

use naia_shared::{
    ComponentFieldUpdate, ComponentKind, ComponentKinds, EntityAndGlobalEntityConverter,
    GlobalWorldManagerType, LocalEntityAndGlobalEntityConverter, PendingComponentUpdate,
    ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicaMutTrait, ReplicaMutWrapper,
    ReplicaRefTrait, ReplicaRefWrapper, Replicate, ReplicatedComponent, SerdeErr, WorldMutType,
    WorldRefType,
};

use super::world_data::WorldData;

// --- downcast adapters for the typed read/write API (off hot path) ---

/// Adapts a `ReplicaDynRefWrapper` to a `ReplicaRefTrait<R>` by downcasting via `Any`.
struct DynRefDowncast<'a, R: Replicate> {
    inner: ReplicaDynRefWrapper<'a>,
    _phantom: PhantomData<R>,
}
impl<'a, R: Replicate> ReplicaRefTrait<R> for DynRefDowncast<'a, R> {
    fn to_ref(&self) -> &R {
        (&*self.inner)
            .to_any()
            .downcast_ref::<R>()
            .expect("DynRefDowncast: component type mismatch")
    }
}

/// Adapts a `ReplicaDynMutWrapper` to a `ReplicaMutTrait<R>` by downcasting via `Any`.
struct DynMutDowncast<'a, R: Replicate> {
    inner: ReplicaDynMutWrapper<'a>,
    _phantom: PhantomData<R>,
}
impl<'a, R: Replicate> ReplicaRefTrait<R> for DynMutDowncast<'a, R> {
    fn to_ref(&self) -> &R {
        (&*self.inner)
            .to_any()
            .downcast_ref::<R>()
            .expect("DynMutDowncast: component type mismatch")
    }
}
impl<'a, R: Replicate> ReplicaMutTrait<R> for DynMutDowncast<'a, R> {
    fn to_mut(&mut self) -> &mut R {
        (&mut *self.inner)
            .to_any_mut()
            .downcast_mut::<R>()
            .expect("DynMutDowncast: component type mismatch (mut)")
    }
}

// WorldProxy

pub trait WorldProxy<'w> {
    fn proxy(self) -> WorldRef<'w>;
}

impl<'w> WorldProxy<'w> for &'w World {
    fn proxy(self) -> WorldRef<'w> {
        WorldRef::new(self)
    }
}

// WorldProxyMut

pub trait WorldProxyMut<'w> {
    fn proxy_mut(self) -> WorldMut<'w>;
}

impl<'w> WorldProxyMut<'w> for &'w mut World {
    fn proxy_mut(self) -> WorldMut<'w> {
        WorldMut::new(self)
    }
}

// WorldRef //

pub struct WorldRef<'w> {
    world: &'w World,
}

impl<'w> WorldRef<'w> {
    pub fn new(world: &'w World) -> Self {
        WorldRef { world }
    }
}

impl<'w> WorldRefType<Entity> for WorldRef<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: ReplicatedComponent>(&self, entity: &Entity) -> bool {
        has_component::<R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
        has_component_of_kind(self.world, entity, component_kind)
    }

    fn component<R: ReplicatedComponent>(
        &'_ self,
        entity: &Entity,
    ) -> Option<ReplicaRefWrapper<'_, R>> {
        component(self.world, entity)
    }

    fn component_of_kind(
        &'_ self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'_>> {
        component_of_kind(self.world, entity, component_kind)
    }
}

// WorldMut

pub struct WorldMut<'w> {
    world: &'w mut World,
}

impl<'w> WorldMut<'w> {
    pub fn new(world: &'w mut World) -> Self {
        Self { world }
    }

    /// Access the raw bevy [`World`].
    ///
    /// Used by `naia-bevy-server` extension methods on
    /// [`naia_server::TickCtx<'_, Entity, WorldMut<'_>>`] that need bevy
    /// primitives not reachable through [`naia_shared::WorldMutType`]
    /// (e.g., draining `Messages<HostSyncEvent>`).
    pub fn bevy_world_mut(&mut self) -> &mut World {
        self.world
    }
}

impl<'w> WorldRefType<Entity> for WorldMut<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: ReplicatedComponent>(&self, entity: &Entity) -> bool {
        has_component::<R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
        has_component_of_kind(self.world, entity, component_kind)
    }

    fn component<R: ReplicatedComponent>(
        &'_ self,
        entity: &Entity,
    ) -> Option<ReplicaRefWrapper<'_, R>> {
        component(self.world, entity)
    }

    fn component_of_kind(
        &'_ self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'_>> {
        component_of_kind(self.world, entity, component_kind)
    }
}

impl<'w> WorldMutType<Entity> for WorldMut<'w> {
    fn spawn_entity(&mut self) -> Entity {
        let entity = self.world.spawn_empty().id();

        let mut world_data = world_data_unchecked_mut(self.world);
        world_data.spawn_entity(&entity);

        entity
    }

    fn local_duplicate_entity(&mut self, entity: &Entity) -> Entity {
        let new_entity = WorldMutType::<Entity>::spawn_entity(self);

        WorldMutType::<Entity>::local_duplicate_components(self, &new_entity, entity);

        new_entity
    }

    fn local_duplicate_components(&mut self, mutable_entity: &Entity, immutable_entity: &Entity) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, immutable_entity) {
            let mut component_copy_opt: Option<Box<dyn Replicate>> = None;
            if let Some(component) = self.component_of_kind(immutable_entity, &component_kind) {
                component_copy_opt = Some(component.copy_to_box());
            }
            if let Some(mut component_copy) = component_copy_opt {
                component_copy.localize();
                self.insert_boxed_component(mutable_entity, component_copy);
            }
        }
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        // Resource-carrier guard (bevy 0.19): a ReplicatedResource carrier
        // entity's `Resource`-derived component ALIASES `Res<R>` (a
        // Component+Resource type shares one storage cell). Despawning such
        // an entity PANICS — once an entity has held a resource component,
        // `World::despawn` errors even after the component is removed. A
        // replicated resource therefore follows bevy's RESOURCE lifecycle,
        // not the entity lifecycle: "despawn" of the carrier is emulated by
        // removing the resource component(s) (clean — turns `Res<R>` into
        // `None`), leaving the now-empty carrier entity in place (reclaimed
        // on world drop). This is the single chokepoint every despawn path
        // funnels through (server `remove_resource`, client disconnect
        // `despawn_all_remote_entities`, scope-exit, server-driven despawn).
        // An entity is a resource carrier if it currently holds a resource
        // component, OR it has ever held one (marked at insert; the resource
        // component may already have been removed by a preceding
        // component-remove op — resource removal replicates as
        // component-remove THEN entity-despawn).
        let resource_kinds_on_entity: Vec<ComponentKind> = {
            let component_kinds = WorldMutType::<Entity>::component_kinds(self, entity);
            let world_data = world_data(self.world);
            component_kinds
                .into_iter()
                .filter(|kind| world_data.is_resource_kind(kind))
                .collect()
        };
        let is_resource_carrier = !resource_kinds_on_entity.is_empty()
            || world_data(self.world).is_resource_carrier_entity(entity);
        if is_resource_carrier {
            // Remove any resource component(s) still present (clean — turns
            // `Res<R>` into `None`), then retain the now-empty entity. We must
            // NOT `World::despawn` a (former) resource carrier.
            for kind in &resource_kinds_on_entity {
                let _ = self.remove_component_of_kind(entity, kind);
            }
            // Drop the entity from naia's adapter-side entity set, but DO
            // NOT call `World::despawn` (the empty carrier entity is
            // retained, reclaimed on world drop).
            let mut world_data = world_data_unchecked_mut(self.world);
            world_data.despawn_entity(entity);
            return;
        }

        let mut world_data = world_data_unchecked_mut(self.world);
        world_data.despawn_entity(entity);

        self.world.despawn(*entity);
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<ComponentKind> {
        let mut kinds = Vec::new();

        let world_data = world_data(self.world);

        let components = self.world.components();

        for component_id in self.world.entity(*entity).archetype().components() {
            let component_info = components
                .get_info(*component_id)
                .expect("Components need info to instantiate");
            let type_id = component_info
                .type_id()
                .expect("Components need type_id to instantiate");
            let component_kind = ComponentKind::from(type_id);

            if world_data.has_kind(&component_kind) {
                kinds.push(component_kind);
            }
        }

        kinds
    }

    fn component_mut<R: ReplicatedComponent>(
        &'_ mut self,
        entity: &Entity,
    ) -> Option<ReplicaMutWrapper<'_, R>> {
        let kind = ComponentKind::of::<R>();
        let dyn_mut = component_mut_of_kind_raw(self.world, entity, &kind)?;
        Some(ReplicaMutWrapper::new(DynMutDowncast {
            inner: dyn_mut,
            _phantom: PhantomData::<R>,
        }))
    }

    fn component_mut_of_kind(
        &'_ mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynMutWrapper<'_>> {
        let world_data = world_data(self.world);
        let component_access = world_data.component_access(component_kind)?;
        let new_component_access = component_access.box_clone();
        new_component_access.component_mut(self.world, entity)
    }

    fn component_apply_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &Entity,
        component_kind: &ComponentKind,
        update: PendingComponentUpdate,
    ) -> Result<(), SerdeErr> {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                if let Some(mut component) = accessor.component_mut(world, entity) {
                    let _update_result = component.read_apply_update(converter, update);
                }
            });
        Ok(())
    }

    fn component_apply_field_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &Entity,
        component_kind: &ComponentKind,
        update: ComponentFieldUpdate,
    ) -> Result<(), SerdeErr> {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                if let Some(mut component) = accessor.component_mut(world, entity) {
                    let _update_result = component.read_apply_field_update(converter, update);
                }
            });
        Ok(())
    }

    fn mirror_entities(&mut self, new_entity: &Entity, old_entity: &Entity) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, old_entity) {
            WorldMutType::<Entity>::mirror_components(
                self,
                new_entity,
                old_entity,
                &component_kind,
            );
        }
    }

    fn mirror_components(
        &mut self,
        mutable_entity: &Entity,
        immutable_entity: &Entity,
        component_kind: &ComponentKind,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.mirror_components(world, mutable_entity, immutable_entity);
            });
    }

    fn insert_component<R: ReplicatedComponent>(&mut self, entity: &Entity, component_ref: R) {
        // Route through the boxed/dynamic path so we don't require R: bevy_ecs::Component
        // (that bound is on ComponentAccessor<R>, not on ReplicatedComponent).
        let boxed: Box<dyn Replicate> = Box::new(component_ref);
        self.insert_boxed_component(entity, boxed);
    }

    fn insert_boxed_component(&mut self, entity: &Entity, boxed_component: Box<dyn Replicate>) {
        let component_kind = boxed_component.kind();
        self.world
            .resource_scope(|world: &mut World, mut data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(&component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.insert_component(world, entity, boxed_component);
                // bevy 0.19: a ReplicatedResource carrier entity must never
                // be `World::despawn`ed. Remember it so the despawn
                // chokepoint routes to component-remove even after the
                // resource component is later removed.
                if data.is_resource_kind(&component_kind) {
                    data.mark_resource_carrier_entity(entity);
                }
            });
    }

    fn remove_component<R: ReplicatedComponent>(&mut self, entity: &Entity) -> Option<R> {
        let kind = ComponentKind::of::<R>();
        let boxed = self.remove_component_of_kind(entity, &kind)?;
        let boxed_any = boxed.to_boxed_any();
        Some(
            *boxed_any
                .downcast::<R>()
                .expect("remove_component: type mismatch"),
        )
    }

    fn remove_component_of_kind(
        &mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        let mut output: Option<Box<dyn Replicate>> = None;
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                output = accessor.remove_component(world, entity);
            });
        output
    }

    fn entity_publish(
        &mut self,
        component_kinds: &ComponentKinds,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &Entity,
    ) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, entity) {
            WorldMutType::<Entity>::component_publish(
                self,
                component_kinds,
                converter,
                global_world_manager,
                entity,
                &component_kind,
            );
        }
    }

    fn component_publish(
        &mut self,
        component_kinds: &ComponentKinds,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_entity: &Entity,
        component_kind: &ComponentKind,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.component_publish(
                    component_kinds,
                    converter,
                    global_world_manager,
                    world,
                    world_entity,
                );
            });
    }

    fn entity_unpublish(&mut self, world_entity: &Entity) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, world_entity) {
            WorldMutType::<Entity>::component_unpublish(self, world_entity, &component_kind);
        }
    }

    fn component_unpublish(&mut self, entity: &Entity, component_kind: &ComponentKind) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.component_unpublish(world, entity);
            });
    }

    fn entity_enable_delegation(
        &mut self,
        component_kinds: &ComponentKinds,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &Entity,
    ) {
        if !WorldRefType::<Entity>::has_entity(self, entity) {
            // Entity was despawned from Bevy's world before the server's delegation-complete
            // message arrived (e.g. client undo removed the entity before the MigrateResponse
            // packet was processed).  Naia will clean it up via on_despawn → despawn_entity_worldless
            // in a later system, so there is nothing to do here.
            return;
        }
        for component_kind in WorldMutType::<Entity>::component_kinds(self, entity) {
            WorldMutType::<Entity>::component_enable_delegation(
                self,
                component_kinds,
                converter,
                global_world_manager,
                entity,
                &component_kind,
            );
        }
    }

    fn component_enable_delegation(
        &mut self,
        component_kinds: &ComponentKinds,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.component_enable_delegation(
                    component_kinds,
                    converter,
                    global_world_manager,
                    world,
                    entity,
                );
            });
    }

    fn entity_disable_delegation(&mut self, entity: &Entity) {
        if !WorldRefType::<Entity>::has_entity(self, entity) {
            return;
        }
        for component_kind in WorldMutType::<Entity>::component_kinds(self, entity) {
            WorldMutType::<Entity>::component_disable_delegation(self, entity, &component_kind);
        }
    }

    fn component_disable_delegation(&mut self, entity: &Entity, component_kind: &ComponentKind) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.component_disable_delegation(world, entity);
            });
    }
}

// private static methods

fn has_entity(world: &World, entity: &Entity) -> bool {
    world.get_entity(*entity).is_ok()
}

fn entities(world: &World) -> Vec<Entity> {
    let world_data = world_data(world);
    world_data.entities()
}

fn has_component<R: ReplicatedComponent>(world: &World, entity: &Entity) -> bool {
    has_component_of_kind(world, entity, &ComponentKind::of::<R>())
}

fn has_component_of_kind(world: &World, entity: &Entity, component_kind: &ComponentKind) -> bool {
    // Fallible lookup, like the sibling `has_component`: events can reference
    // an entity despawned earlier in the same batch (e.g. a disconnect
    // cascade), and a despawned entity trivially has no components. The
    // infallible `world.entity()` panics the whole app on that race.
    let Ok(entity_ref) = world.get_entity(*entity) else {
        return false;
    };
    entity_ref.contains_type_id(<ComponentKind as Into<TypeId>>::into(*component_kind))
}

fn component<'a, R: ReplicatedComponent>(
    world: &'a World,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, R>> {
    let kind = ComponentKind::of::<R>();
    let dyn_ref = component_of_kind(world, entity, &kind)?;
    Some(ReplicaRefWrapper::new(DynRefDowncast {
        inner: dyn_ref,
        _phantom: PhantomData::<R>,
    }))
}

fn component_of_kind<'a>(
    world: &'a World,
    entity: &Entity,
    component_kind: &ComponentKind,
) -> Option<ReplicaDynRefWrapper<'a>> {
    let world_data = world_data(world);
    let Some(component_access) = world_data.component_access(component_kind) else {
        panic!("ComponentKind has not been registered?");
    };
    component_access.component(world, entity)
}

fn world_data(world: &World) -> &WorldData {
    world
        .get_resource::<WorldData>()
        .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!")
}

fn component_mut_of_kind_raw<'a>(
    world: &'a mut World,
    entity: &Entity,
    component_kind: &ComponentKind,
) -> Option<ReplicaDynMutWrapper<'a>> {
    let component_access = {
        let world_data = world_data(world);
        let Some(accessor) = world_data.component_access(component_kind) else {
            panic!("ComponentKind has not been registered?");
        };
        accessor.box_clone()
    };
    component_access.component_mut(world, entity)
}

fn world_data_unchecked_mut(world: &'_ mut World) -> Mut<'_, WorldData> {
    // Safety: We have exclusive access via &mut World. as_unsafe_world_cell() is used here
    // because get_resource_mut() requires UnsafeWorldCell; no other borrow of WorldData
    // is alive at the call site. The returned Mut<'_> is tied to the world's lifetime.
    unsafe {
        world
            .as_unsafe_world_cell()
            .get_resource_mut::<WorldData>()
            .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!")
    }
}
