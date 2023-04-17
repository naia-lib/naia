use std::{any::Any, collections::HashMap};

use naia_shared::{
    BigMap, ComponentKind, ComponentUpdate, LocalEntityAndGlobalEntityConverter,
    ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, Replicate,
    SerdeErr, WorldMutType, WorldRefType,
};

use super::{
    component_ref::{ComponentMut, ComponentRef},
    entity::Entity,
};

// World //

/// A default World which implements WorldRefType/WorldMutType and that Naia can
/// use to store Entities/Components.
/// It's recommended to use this only when you do not have another ECS library's
/// own World available.
pub struct World {
    pub entities: BigMap<Entity, HashMap<ComponentKind, Box<dyn Replicate>>>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            entities: BigMap::new(),
        }
    }
}

impl World {
    /// Convert to WorldRef
    pub fn proxy<'w>(&'w self) -> WorldRef<'w> {
        WorldRef::<'w>::new(self)
    }

    /// Convert to WorldMut
    pub fn proxy_mut<'w>(&'w mut self) -> WorldMut<'w> {
        WorldMut::<'w>::new(self)
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

// WorldMut //

pub struct WorldMut<'w> {
    world: &'w mut World,
}

impl<'w> WorldMut<'w> {
    pub fn new(world: &'w mut World) -> Self {
        WorldMut { world }
    }
}

// WorldRefType //

impl<'w> WorldRefType<Entity> for WorldRef<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: Replicate>(&self, entity: &Entity) -> bool {
        has_component::<R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
        has_component_of_type(self.world, entity, component_kind)
    }

    fn component<R: Replicate>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        component_of_kind(self.world, entity, component_kind)
    }
}

impl<'w> WorldRefType<Entity> for WorldMut<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: Replicate>(&self, entity: &Entity) -> bool {
        has_component::<R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
        has_component_of_type(self.world, entity, component_kind)
    }

    fn component<R: Replicate>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        component_of_kind(self.world, entity, component_kind)
    }
}

impl<'w> WorldMutType<Entity> for WorldMut<'w> {
    fn spawn_entity(&mut self) -> Entity {
        let component_map = HashMap::new();
        self.world.entities.insert(component_map)
    }

    fn duplicate_entity(&mut self, entity: &Entity) -> Entity {
        let new_entity = self.spawn_entity();

        self.duplicate_components(&new_entity, entity);

        new_entity
    }

    fn duplicate_components(&mut self, new_entity: &Entity, old_entity: &Entity) {
        for component_kind in self.component_kinds(old_entity) {
            let mut boxed_option: Option<Box<dyn Replicate>> = None;
            if let Some(component) = self.component_of_kind(old_entity, &component_kind) {
                boxed_option = Some(component.copy_to_box());
            }
            if let Some(boxed_component) = boxed_option {
                self.insert_boxed_component(new_entity, boxed_component);
            } else {
                panic!("this shouldn't happen");
            }
        }
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        self.world.entities.remove(entity);
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<ComponentKind> {
        let mut output: Vec<ComponentKind> = Vec::new();

        if let Some(component_map) = self.world.entities.get(entity) {
            for component_kind in component_map.keys() {
                output.push(*component_kind);
            }
        }

        output
    }

    fn component_mut<R: Replicate>(&mut self, entity: &Entity) -> Option<ReplicaMutWrapper<R>> {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            if let Some(boxed_component) = component_map.get_mut(&ComponentKind::of::<R>()) {
                if let Some(raw_ref) = boxed_component.to_any_mut().downcast_mut::<R>() {
                    let wrapper = ComponentMut::<R>::new(raw_ref);
                    let wrapped_ref = ReplicaMutWrapper::new(wrapper);
                    return Some(wrapped_ref);
                }
            }
        }

        None
    }

    fn component_mut_of_kind<'a>(
        &'a mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynMutWrapper<'a>> {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            if let Some(boxed_component) = component_map.get_mut(&component_kind) {
                return Some(ReplicaDynMutWrapper::new(boxed_component.dyn_mut()));
            }
        }

        None
    }

    fn component_apply_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &Entity,
        component_kind: &ComponentKind,
        update: ComponentUpdate,
    ) -> Result<(), SerdeErr> {
        if let Some(mut component) = component_mut_of_kind(self.world, entity, component_kind) {
            component.read_apply_update(converter, update)?;
        }
        Ok(())
    }

    fn mirror_entities(&mut self, new_entity: &Entity, old_entity: &Entity) {
        for component_kind in self.component_kinds(old_entity) {
            self.mirror_components(new_entity, old_entity, &component_kind);
        }
    }

    fn mirror_components(
        &mut self,
        mutable_entity: &Entity,
        immutable_entity: &Entity,
        component_kind: &ComponentKind,
    ) {
        let immutable_component_opt: Option<Box<dyn Replicate>> = {
            if let Some(immutable_component_map) = self.world.entities.get(immutable_entity) {
                if let Some(immutable_component) = immutable_component_map.get(component_kind) {
                    Some(immutable_component.copy_to_box())
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(immutable_component) = immutable_component_opt {
            if let Some(mutable_component_map) = self.world.entities.get_mut(mutable_entity) {
                if let Some(mutable_component) = mutable_component_map.get_mut(component_kind) {
                    mutable_component.mirror(immutable_component.as_ref());
                }
            }
        }
    }

    fn insert_component<R: Replicate>(&mut self, entity: &Entity, component: R) {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            let component_kind = ComponentKind::of::<R>();
            if component_map.contains_key(&component_kind) {
                panic!("Entity already has a Component of that type!");
            }
            component_map.insert(component_kind, Box::new(component));
        }
    }

    fn insert_boxed_component(&mut self, entity: &Entity, boxed_component: Box<dyn Replicate>) {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            let component_kind = boxed_component.kind();
            if component_map.contains_key(&component_kind) {
                panic!("Entity already has a Component of that type!");
            }
            component_map.insert(component_kind, boxed_component);
        }
    }

    fn remove_component<R: Replicate>(&mut self, entity: &Entity) -> Option<R> {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            if let Some(boxed_component) = component_map.remove(&ComponentKind::of::<R>()) {
                return Box::<dyn Any + 'static>::downcast::<R>(boxed_component.to_boxed_any())
                    .ok()
                    .map(|boxed_c| *boxed_c);
            }
        }
        None
    }

    fn remove_component_of_kind(
        &mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            return component_map.remove(component_kind);
        }

        None
    }
}

// private methods //

fn has_entity(world: &World, entity: &Entity) -> bool {
    world.entities.contains_key(entity)
}

fn entities(world: &World) -> Vec<Entity> {
    let mut output = Vec::new();

    for (key, _) in world.entities.iter() {
        output.push(key);
    }

    output
}

fn has_component<R: Replicate>(world: &World, entity: &Entity) -> bool {
    if let Some(component_map) = world.entities.get(entity) {
        return component_map.contains_key(&ComponentKind::of::<R>());
    }

    false
}

fn has_component_of_type(world: &World, entity: &Entity, component_kind: &ComponentKind) -> bool {
    if let Some(component_map) = world.entities.get(entity) {
        return component_map.contains_key(component_kind);
    }

    false
}

fn component<'a, R: Replicate>(
    world: &'a World,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, R>> {
    if let Some(component_map) = world.entities.get(entity) {
        if let Some(boxed_component) = component_map.get(&ComponentKind::of::<R>()) {
            if let Some(raw_ref) = boxed_component.to_any().downcast_ref::<R>() {
                let wrapper = ComponentRef::<R>::new(raw_ref);
                let wrapped_ref = ReplicaRefWrapper::new(wrapper);
                return Some(wrapped_ref);
            }
        }
    }

    None
}

fn component_of_kind<'a>(
    world: &'a World,
    entity: &Entity,
    component_kind: &ComponentKind,
) -> Option<ReplicaDynRefWrapper<'a>> {
    if let Some(component_map) = world.entities.get(entity) {
        if let Some(component) = component_map.get(component_kind) {
            return Some(ReplicaDynRefWrapper::new(component.dyn_ref()));
        }
    }

    None
}

fn component_mut_of_kind<'a>(
    world: &'a mut World,
    entity: &Entity,
    component_kind: &ComponentKind,
) -> Option<ReplicaDynMutWrapper<'a>> {
    if let Some(component_map) = world.entities.get_mut(entity) {
        if let Some(raw_ref) = component_map.get_mut(component_kind) {
            let wrapped_ref = ReplicaDynMutWrapper::new(raw_ref.dyn_mut());
            return Some(wrapped_ref);
        }
    }

    None
}
