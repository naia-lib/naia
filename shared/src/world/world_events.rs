use std::{collections::HashMap, marker::PhantomData, mem, vec::IntoIter};

use crate::{ComponentKind, Replicate, Tick};

pub struct WorldEvents<E: Copy> {
    pub spawns: Vec<E>,
    pub despawns: Vec<E>,
    inserts: HashMap<ComponentKind, Vec<E>>,
    removes: HashMap<ComponentKind, Vec<(E, Box<dyn Replicate>)>>,
    updates: HashMap<ComponentKind, Vec<(Tick, E)>>,
    empty: bool,
}

impl<E: Copy> WorldEvents<E> {
    pub fn new() -> Self {
        Self {
            spawns: Vec::new(),
            despawns: Vec::new(),
            inserts: HashMap::new(),
            removes: HashMap::new(),
            updates: HashMap::new(),
            empty: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn has<V: WorldEvent<E>>(&mut self) -> bool {
        return V::has(self);
    }

    pub fn read<V: WorldEvent<E>>(&mut self) -> V::Iter {
        return V::iter(self);
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_inserts(&self) -> bool {
        !self.inserts.is_empty()
    }
    pub fn take_inserts(&mut self) -> Option<HashMap<ComponentKind, Vec<E>>> {
        if self.inserts.is_empty() {
            return None;
        } else {
            return Some(mem::take(&mut self.inserts));
        }
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_updates(&self) -> bool {
        !self.updates.is_empty()
    }
    pub fn take_updates(&mut self) -> Option<HashMap<ComponentKind, Vec<(Tick, E)>>> {
        if self.updates.is_empty() {
            return None;
        } else {
            return Some(mem::take(&mut self.updates));
        }
    }

    // These method are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_removes(&self) -> bool {
        !self.removes.is_empty()
    }
    pub fn take_removes(&mut self) -> Option<HashMap<ComponentKind, Vec<(E, Box<dyn Replicate>)>>> {
        if self.removes.is_empty() {
            return None;
        } else {
            return Some(mem::take(&mut self.removes));
        }
    }

    pub(crate) fn push_spawn(&mut self, entity: E) {
        self.spawns.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_despawn(&mut self, entity: E) {
        self.despawns.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_insert(&mut self, entity: E, component_kind: ComponentKind) {
        if !self.inserts.contains_key(&component_kind) {
            self.inserts.insert(component_kind, Vec::new());
        }
        let list = self.inserts.get_mut(&component_kind).unwrap();
        list.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_remove(&mut self, entity: E, component: Box<dyn Replicate>) {
        let component_kind: ComponentKind = component.kind();
        if !self.removes.contains_key(&component_kind) {
            self.removes.insert(component_kind, Vec::new());
        }
        let list = self.removes.get_mut(&component_kind).unwrap();
        list.push((entity, component));
        self.empty = false;
    }

    pub(crate) fn push_update(&mut self, tick: Tick, entity: E, component_kind: ComponentKind) {
        if !self.updates.contains_key(&component_kind) {
            self.updates.insert(component_kind, Vec::new());
        }
        let list = self.updates.get_mut(&component_kind).unwrap();
        list.push((tick, entity));
        self.empty = false;
    }

    pub fn clear(&mut self) {
        self.spawns.clear();
        self.despawns.clear();
        self.inserts.clear();
        self.removes.clear();
        self.updates.clear();
        self.empty = true;
    }
}

// Event Trait
pub trait WorldEvent<E: Copy> {
    type Iter;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter;

    fn has(events: &WorldEvents<E>) -> bool;
}

// Spawn Event
pub struct SpawnEntityEvent;
impl<E: Copy> WorldEvent<E> for SpawnEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.spawns);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.spawns.is_empty()
    }
}

// Despawn Event
pub struct DespawnEntityEvent;
impl<E: Copy> WorldEvent<E> for DespawnEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.despawns);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.despawns.is_empty()
    }
}

// Insert Event
pub struct InsertComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Copy, C: Replicate> WorldEvent<E> for InsertComponentEvent<C> {
    type Iter = IntoIter<E>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.inserts.remove(&component_kind) {
            return IntoIterator::into_iter(boxed_list);
        }

        return IntoIterator::into_iter(Vec::new());
    }

    fn has(events: &WorldEvents<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.inserts.contains_key(&component_kind)
    }
}

// Update Event
pub struct UpdateComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Copy, C: Replicate> WorldEvent<E> for UpdateComponentEvent<C> {
    type Iter = IntoIter<(Tick, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.updates.remove(&component_kind) {
            return IntoIterator::into_iter(boxed_list);
        }

        return IntoIterator::into_iter(Vec::new());
    }

    fn has(events: &WorldEvents<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.updates.contains_key(&component_kind)
    }
}

// Remove Event
pub struct RemoveComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Copy, C: Replicate> WorldEvent<E> for RemoveComponentEvent<C> {
    type Iter = IntoIter<(E, C)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.removes.remove(&component_kind) {
            let mut output_list: Vec<(E, C)> = Vec::new();

            for (entity, boxed_component) in boxed_list {
                let boxed_any = boxed_component.to_boxed_any();
                let component = boxed_any.downcast::<C>().unwrap();
                output_list.push((entity, *component));
            }

            return IntoIterator::into_iter(output_list);
        }

        return IntoIterator::into_iter(Vec::new());
    }

    fn has(events: &WorldEvents<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.removes.contains_key(&component_kind)
    }
}
