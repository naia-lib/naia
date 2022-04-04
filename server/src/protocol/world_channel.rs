use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::time::Duration;

use naia_shared::{Protocolize, ProtocolKindType};
use crate::protocol::entity_action::EntityAction;
use crate::protocol::entity_manager::ActionId;
use crate::protocol::user_diff_handler::UserDiffHandler;
use crate::sequence_list::SequenceList;
use crate::server::Instant;

const RESEND_ACTION_RTT_FACTOR: f32 = 1.5;

// ComponentChannel

pub enum ComponentChannel {
    Inserting,
    Inserted,
    Removing,
}

// EntityChannel

pub enum EntityChannel<K: ProtocolKindType> {
    Spawning,
    Spawned(CheckedMap<K, ComponentChannel>),
    Despawning,
}

// WorldChannel

pub struct WorldChannel<E: Copy + Eq + Hash, K: ProtocolKindType> {
    host_world: CheckedMap<E, CheckedSet<K>>,
    remote_world: CheckedMap<E, CheckedSet<K>>,
    entity_channels: CheckedMap<E, EntityChannel<K>>,
    outgoing_actions: VecDeque<(ActionId, Option<(Option<Instant>, EntityAction<E, K>)>)>,
    next_action_id: ActionId,
}

impl<E: Copy + Eq + Hash, K: ProtocolKindType> WorldChannel<E, K> {
    pub fn new() -> Self {
        Self {
            host_world: CheckedMap::new(),
            remote_world: CheckedMap::new(),
            entity_channels: CheckedMap::new(),
            outgoing_actions: VecDeque::new(),
            next_action_id: 0,
        }
    }

    pub fn action_delivered(&self, action_id: &ActionId) {
        todo!()
    }

    fn new_action_id(&mut self) -> ActionId {
        let output = self.next_action_id;
        self.next_action_id = self.next_action_id.wrapping_add(1);
        output
    }

    // Main

    pub fn host_has_entity(&self, entity: &E) -> bool {
        return self.host_world.contains_key(entity);
    }

    pub fn entity_channel_is_open(&self, entity: &E) -> bool {
        return if let Some(EntityChannel::Spawned(_)) = self.entity_channels.get(entity) {
            true
        } else {
            false
        }
    }

    // Host Updates

    pub fn host_spawn_entity(&mut self, entity: &E) {
        if self.host_world.contains_key(entity) {
            // do nothing
            return;
        }

        self.host_world.insert(*entity, CheckedSet::new());

        // NEW ACTION
        self.entity_channels.insert(*entity, EntityChannel::Spawning);
    }

    pub fn host_despawn_entity(&mut self, entity: &E) {
        if !self.host_world.contains_key(entity) {
            // do nothing
            return;
        }

        self.host_world.remove(entity);

        if let Some(EntityChannel::Spawned(_)) = self.entity_channels.get(entity) {
            // NEW ACTION
            self.entity_channels.insert(*entity, EntityChannel::Despawning);
        }
    }

    pub fn host_insert_component(&mut self, entity: &E, component: &K) {
        if !self.host_world.contains_key(entity) {
            panic!("cannot insert component into non-existent entity");
        }

        let components = self.host_world.get_mut(entity).unwrap();
        if components.contains(component) {
            // do nothing
            return;
        }

        components.insert(*component);

        if let EntityChannel::Spawned(component_channels) = self.entity_channels.get_mut(entity).unwrap() {
            // NEW ACTION
            component_channels.insert(*component, ComponentChannel::Inserting);
        }
    }

    pub fn host_remove_component(&mut self, entity: &E, component: &K) {
        if !self.host_world.contains_key(entity) {
            panic!("cannot remove component from non-existent entity");
        }

        let components = self.host_world.get_mut(entity).unwrap();
        if !components.contains(component) {
            // do nothing
            return;
        }

        components.remove(component);

        if let EntityChannel::Spawned(component_channels) = self.entity_channels.get_mut(entity).unwrap() {
            if let ComponentChannel::Inserted = component_channels.get(component).unwrap() {
                // NEW ACTION
                component_channels.insert(*component, ComponentChannel::Removing);
            }

        }
    }

    // Remote Updates

    pub fn remote_spawn_entity(&mut self, entity: &E) {
        if self.remote_world.contains_key(entity) {
            // do nothing
            return;
        }

        self.remote_world.insert(*entity, CheckedSet::new());

        if !self.entity_channels.contains_key(entity) {
            // NEW ACTION
            self.entity_channels.insert(*entity, EntityChannel::Spawning);
        }
    }

    pub fn remote_despawn_entity(&mut self, entity: &E) {

    }

    pub fn remote_insert_component(&mut self, entity: &E, component: &K) {

    }

    pub fn remote_remove_component(&mut self, entity: &E, component: &K) {

    }

    // Collect

    pub fn collect_next_actions(&mut self, now: &Instant, rtt_millis: &f32) -> Vec<(ActionId, EntityAction<E, K>)> {

        let mut output = Vec::<(ActionId, EntityAction<E, K>)>::new();

        let resend_duration = Duration::from_millis((RESEND_ACTION_RTT_FACTOR * rtt_millis) as u64);

        // go through sending actions, if we haven't sent in a while, add message to
        // outgoing queue
        for (action_id, action_opt) in self.outgoing_actions.iter_mut() {
            if let Some((last_sent_opt, action)) = action_opt {
                // check whether we should send outgoing actions in the next packet
                let mut should_send = false;

                if let Some(last_sent) = last_sent_opt {
                    if last_sent.elapsed() > resend_duration {
                        should_send = true;
                    }
                } else {
                    should_send = true;
                }

                if !should_send {
                    continue;
                }

                // put action into outgoing queue
                output.push((*action_id, action.clone()));

                *last_sent_opt = Some(now.clone());
            }
        }

        output
    }

    pub fn collect_next_updates(&self, diff_handler: &UserDiffHandler<E, K>) -> HashMap<E, HashSet<K>> {

        let mut output = HashMap::new();

        for (entity, entity_channel) in self.entity_channels.iter() {
            if let EntityChannel::Spawned(component_channels) = entity_channel {
                for (component, component_channel) in component_channels.iter() {
                    if let ComponentChannel::Inserted = component_channel {

                        if diff_handler.diff_mask_is_clear(entity, component)
                        {
                            // no updates detected, do nothing
                            continue;
                        }

                        if !output.contains_key(entity) {
                            output.insert(*entity, HashSet::new());
                        }
                        let send_component_set = output.get_mut(entity).unwrap();
                        send_component_set.insert(*component);

                    }
                }
            }
        }

        output
    }
}

// CheckedMap
pub struct CheckedMap<K: Eq + Hash, V> {
    map: HashMap<K, V>
}

impl<K: Eq + Hash, V> CheckedMap<K, V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        return self.map.contains_key(key);
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        return self.map.get(key);
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        return self.map.get_mut(key);
    }

    pub fn insert(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            panic!("Cannot insert and replace value for given key. Check first.")
        }

        self.map.insert(key, value);
    }

    pub fn remove(&mut self, key: &K) {
        if !self.map.contains_key(&key) {
            panic!("Cannot remove value for key with non-existent value. Check whether map contains key first.")
        }

        self.map.remove(key);
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<K, V> {
        return self.map.iter();
    }
}

// CheckedSet
pub struct CheckedSet<K: Eq + Hash> {
    set: HashSet<K>
}

impl<K: Eq + Hash> CheckedSet<K> {
    pub fn new() -> Self {
        Self {
            set: HashSet::new(),
        }
    }

    pub fn contains(&self, key: &K) -> bool {
        return self.set.contains(key);
    }

    pub fn insert(&mut self, key: K) {
        if self.set.contains(&key) {
            panic!("Cannot insert and replace given key. Check first.")
        }

        self.set.insert(key);
    }

    pub fn remove(&mut self, key: &K) {
        if !self.set.contains(key) {
            panic!("Cannot remove given non-existent key. Check first.")
        }

        self.set.remove(key);
    }
}