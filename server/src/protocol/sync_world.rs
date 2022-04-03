use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use naia_shared::ProtocolKindType;

pub enum ComponentDiffAction {
    Insert,
    Remove,
}

pub enum EntityDiffAction<K: ProtocolKindType> {
    Spawn,
    Despawn,
    SyncComponents(HashMap<K, ComponentDiffAction>)
}

pub enum EntityDiffResult {
    Spawn,
    Despawn,
    EqualAndExists,
    EqualAndEmpty,
}

pub enum ComponentDiffResult {
    Insert,
    Remove,
    Equal,
    Invalid,
}

#[derive(PartialEq)]
pub enum UpdateResult {
    Changed,
    Unchanged,
}

struct MiniWorld<E: Copy + Eq + Hash, K: ProtocolKindType> {
    world: HashMap<E, HashSet<K>>
}

impl<E: Copy + Eq + Hash, K: ProtocolKindType> MiniWorld<E, K> {
    pub fn new() -> Self {
        Self {
            world: HashMap::new(),
        }
    }

    pub fn spawn_entity(&mut self, entity: &E) -> UpdateResult {
        if self.world.contains_key(entity) {
            return UpdateResult::Unchanged;
        }

        self.world.insert(*entity, HashSet::new());
        return UpdateResult::Changed;
    }

    pub fn despawn_entity(&mut self, entity: &E) -> UpdateResult {
        if !self.world.contains_key(entity) {
            return UpdateResult::Unchanged;
        }
        self.world.remove(entity);
        return UpdateResult::Changed;
    }

    pub fn insert_component(&mut self, entity: &E, component: &K) -> UpdateResult {
        if !self.world.contains_key(entity) {
            panic!("Should not insert component unless sure the world already contains the entity");
        }

        let components = self.world.get_mut(entity).unwrap();
        if components.contains(component) {
            return UpdateResult::Unchanged;
        }

        components.insert(*component);
        return UpdateResult::Changed;
    }

    pub fn remove_component(&mut self, entity: &E, component: &K) -> UpdateResult {
        if !self.world.contains_key(entity) {
            panic!("Should not remove component unless sure the world already contains the entity");
        }

        let components = self.world.get_mut(entity).unwrap();
        if !components.contains(component) {
            return UpdateResult::Unchanged;
        }

        components.remove(component);
        return UpdateResult::Changed;
    }

    pub fn has_entity(&self, entity: &E) -> bool {
        return self.world.contains_key(entity);
    }

    pub fn has_component(&self, entity: &E, component: &K) -> bool {
        match self.world.get(entity) {
            None => false,
            Some(components) => components.contains(component),
        }
    }

    pub fn entity_diff(&self, other: &Self, entity: &E) -> EntityDiffResult {
        let self_has_entity = self.has_entity(entity);
        let other_has_entity = other.has_entity(entity);

        if self_has_entity {
            if other_has_entity {
                return EntityDiffResult::EqualAndExists;
            } else {
                return EntityDiffResult::Spawn;
            }
        } else {
            if other_has_entity {
                return EntityDiffResult::Despawn;
            } else {
                return EntityDiffResult::EqualAndEmpty;
            }
        }
    }

    pub fn component_diff(&self, other: &Self, entity: &E, component: &K) -> ComponentDiffResult {
        match self.entity_diff(other, entity) {
            EntityDiffResult::Spawn | EntityDiffResult::Despawn | EntityDiffResult::EqualAndEmpty => ComponentDiffResult::Invalid,
            EntityDiffResult::EqualAndExists => {
                let self_has_component = self.has_component(entity, component);
                let other_has_component = other.has_component(entity, component);

                if self_has_component == other_has_component {
                    ComponentDiffResult::Equal
                } else {
                    if self_has_component {
                        ComponentDiffResult::Insert
                    } else {
                        ComponentDiffResult::Remove
                    }
                }
            }
        }
    }
}

pub struct SyncWorld<E: Copy + Eq + Hash, K: ProtocolKindType> {
    host_world: MiniWorld<E, K>,
    remote_world: MiniWorld<E, K>,
    diff_actions: HashMap<E, EntityDiffAction<K>>,
}

impl<E: Copy + Eq + Hash, K: ProtocolKindType> SyncWorld<E, K> {
    pub fn new() -> Self {
        Self {
            host_world: MiniWorld::new(),
            remote_world: MiniWorld::new(),
            diff_actions: HashMap::new(),
        }
    }

    // Host Updates

    pub fn host_spawn_entity(&mut self, entity: &E) {
        if self.host_world.spawn_entity(entity) == UpdateResult::Unchanged {
            return;
        }

        self.update_diff_entity(entity);
    }

    pub fn host_despawn_entity(&mut self, entity: &E) {
        if self.host_world.despawn_entity(entity) == UpdateResult::Unchanged {
            return;
        }

        self.update_diff_entity(entity);
    }

    pub fn host_insert_component(&mut self, entity: &E, component: &K) {
        if self.host_world.insert_component(entity, component) == UpdateResult::Unchanged {
            return;
        }

        self.update_diff_component(entity, component);
    }

    pub fn host_remove_component(&mut self, entity: &E, component: &K) {
        if self.host_world.remove_component(entity, component) == UpdateResult::Unchanged {
            return;
        }

        self.update_diff_component(entity, component);
    }

    // Remote Updates

    pub fn remote_spawn_entity(&mut self, entity: &E) {
        if self.remote_world.spawn_entity(entity) == UpdateResult::Unchanged {
            return;
        }

        self.update_diff_entity(entity);
    }

    pub fn remote_despawn_entity(&mut self, entity: &E) {
        if self.remote_world.despawn_entity(entity) == UpdateResult::Unchanged {
            return;
        }

        self.update_diff_entity(entity);
    }

    pub fn remote_insert_component(&mut self, entity: &E, component: &K) {
        if self.remote_world.insert_component(entity, component) == UpdateResult::Unchanged {
            return;
        }

        self.update_diff_component(entity, component);
    }

    pub fn remote_remove_component(&mut self, entity: &E, component: &K) {
        if self.remote_world.remove_component(entity, component) == UpdateResult::Unchanged {
            return;
        }

        self.update_diff_component(entity, component);
    }

    // Update Diff

    fn update_diff_entity(&mut self, entity: &E) {
        // this happens after a world spawns/despawns an entity
        match self.host_world.entity_diff(&self.remote_world, entity) {
            EntityDiffResult::Spawn => {
                self.diff_actions.insert(*entity, EntityDiffAction::Spawn);
            }
            EntityDiffResult::Despawn => {
                self.diff_actions.insert(*entity, EntityDiffAction::Despawn);
            }
            EntityDiffResult::EqualAndEmpty => {
                self.diff_actions.remove(entity);
            }
            EntityDiffResult::EqualAndExists => {
                match self.diff_actions.get(entity) {
                    Some(EntityDiffAction::Spawn) | Some(EntityDiffAction::Despawn) => {
                        self.diff_actions.remove(entity);
                    }
                    _ => {}
                }
            }
        }
    }

    fn update_diff_component(&mut self, entity: &E, component: &K) {
        // this happens after a world inserts/removes a component
        let mut remove_entity_action = false;
        match self.diff_actions.get_mut(entity) {
            Some(EntityDiffAction::Spawn) | Some(EntityDiffAction::Despawn) => {
                // do nothing
            }
            Some(EntityDiffAction::SyncComponents(components)) => {
                match self.host_world.component_diff(&self.remote_world, entity, component) {
                    ComponentDiffResult::Equal => {
                        components.remove(component);
                        if components.len() == 0 {
                            remove_entity_action = true;
                        }
                    }
                    ComponentDiffResult::Insert => {
                        components.insert(*component, ComponentDiffAction::Insert);
                    }
                    ComponentDiffResult::Remove => {
                        components.insert(*component, ComponentDiffAction::Remove);
                    }
                    ComponentDiffResult::Invalid => {
                        panic!("should not arrive here");
                    }
                }
            }
            None => {
                match self.host_world.component_diff(&self.remote_world, entity, component) {
                    ComponentDiffResult::Equal => {}
                    ComponentDiffResult::Invalid => {
                        panic!("should not arrive here");
                    }
                    ComponentDiffResult::Insert => {
                        let mut components = HashMap::new();
                        components.insert(*component, ComponentDiffAction::Insert);
                        self.diff_actions.insert(*entity, EntityDiffAction::SyncComponents(components));
                    }
                    ComponentDiffResult::Remove => {
                        let mut components = HashMap::new();
                        components.insert(*component, ComponentDiffAction::Remove);
                        self.diff_actions.insert(*entity, EntityDiffAction::SyncComponents(components));
                    }
                }
            }
        }
        if remove_entity_action {
            self.diff_actions.remove(entity);
        }
    }
}