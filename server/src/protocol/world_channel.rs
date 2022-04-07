use std::{
    collections::{hash_set::Iter, HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use crate::{
    protocol::{
        entity_manager::ActionId,
        entity_message_waitlist::EntityMessageWaitlist, global_diff_handler::GlobalDiffHandler,
        user_diff_handler::UserDiffHandler,
    },
    server::Instant,
};
use naia_shared::{ChannelIndex, ChannelSender, KeyGenerator, NetEntity, ProtocolKindType, Protocolize, ReliableSender, EntityActionReceiver, EntityAction};

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

pub struct WorldChannel<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> {
    host_world: CheckedMap<E, CheckedSet<P::Kind>>,
    remote_world: CheckedMap<E, CheckedSet<P::Kind>>,
    entity_channels: CheckedMap<E, EntityChannel<P::Kind>>,
    outgoing_actions: ReliableSender<EntityAction<E, P::Kind>>,
    delivered_actions: EntityActionReceiver<E, P::Kind>,

    address: SocketAddr,
    pub diff_handler: UserDiffHandler<E, P::Kind>,
    net_entity_generator: KeyGenerator<NetEntity>,
    entity_to_net_entity_map: HashMap<E, NetEntity>,
    net_entity_to_entity_map: HashMap<NetEntity, E>,
    pub delayed_entity_messages: EntityMessageWaitlist<P, E, C>,
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> WorldChannel<P, E, C> {
    pub fn new(
        address: SocketAddr,
        diff_handler: &Arc<RwLock<GlobalDiffHandler<E, P::Kind>>>,
    ) -> Self {
        Self {
            host_world: CheckedMap::new(),
            remote_world: CheckedMap::new(),
            entity_channels: CheckedMap::new(),
            outgoing_actions: ReliableSender::new(RESEND_ACTION_RTT_FACTOR),
            delivered_actions: EntityActionReceiver::new(),

            address,
            diff_handler: UserDiffHandler::new(diff_handler),
            net_entity_generator: KeyGenerator::new(),
            net_entity_to_entity_map: HashMap::new(),
            entity_to_net_entity_map: HashMap::new(),
            delayed_entity_messages: EntityMessageWaitlist::new(),
        }
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
        };
    }

    // Host Updates

    pub fn host_spawn_entity(&mut self, entity: &E) {
        if self.host_world.contains_key(entity) {
            // do nothing
            return;
        }

        self.host_world.insert(*entity, CheckedSet::new());

        if let None = self.entity_channels.get(entity) {
            // spawn entity
            self.entity_channels
                .insert(*entity, EntityChannel::Spawning);
            self.outgoing_actions
                .send_message(EntityAction::SpawnEntity(*entity));
            self.on_entity_channel_opening(entity);
        }
    }

    pub fn host_despawn_entity(&mut self, entity: &E) {
        if !self.host_world.contains_key(entity) {
            // do nothing
            return;
        }

        self.host_world.remove(entity);

        let mut despawn = false;
        let mut removing_components = Vec::new();

        if let Some(EntityChannel::Spawned(component_channels)) = self.entity_channels.get(entity) {
            despawn = true;

            for (component, component_channel) in component_channels.iter() {
                if let ComponentChannel::Inserted = component_channel {
                    removing_components.push(*component);
                }
            }
        }

        if despawn {
            self.entity_channels.remove(entity);
            self.entity_channels
                .insert(*entity, EntityChannel::Despawning);
            self.outgoing_actions
                .send_message(EntityAction::DespawnEntity(*entity));
            self.on_entity_channel_closing(entity);

            for component in removing_components {
                self.on_component_channel_closing(entity, &component);
            }
        }
    }

    pub fn host_insert_component(&mut self, entity: &E, component: &P::Kind) {
        if !self.host_world.contains_key(entity) {
            panic!("cannot insert component into non-existent entity");
        }

        let components = self.host_world.get_mut(entity).unwrap();
        if components.contains(component) {
            // do nothing
            return;
        }

        components.insert(*component);

        if let Some(EntityChannel::Spawned(component_channels)) =
            self.entity_channels.get_mut(entity)
        {
            if let None = component_channels.get(component) {
                // insert component
                component_channels.insert(*component, ComponentChannel::Inserting);
                self.outgoing_actions
                    .send_message(EntityAction::InsertComponent(*entity, *component));
            }
        }
    }

    pub fn host_remove_component(&mut self, entity: &E, component: &P::Kind) {
        if !self.host_world.contains_key(entity) {
            panic!("cannot remove component from non-existent entity");
        }

        let components = self.host_world.get_mut(entity).unwrap();
        if !components.contains(component) {
            // do nothing
            return;
        }

        components.remove(component);

        if let Some(EntityChannel::Spawned(component_channels)) =
            self.entity_channels.get_mut(entity)
        {
            if let Some(ComponentChannel::Inserted) = component_channels.get(component) {
                component_channels.remove(component);

                // remove component
                component_channels.insert(*component, ComponentChannel::Removing);
                self.outgoing_actions
                    .send_message(EntityAction::RemoveComponent(*entity, *component));
                self.on_component_channel_closing(entity, component);
            }
        }
    }

    // Remote Actions

    pub fn remote_spawn_entity(&mut self, entity: &E) {
        if self.remote_world.contains_key(entity) {
            panic!("should not be able to replace entity in remote world");
        }

        if let Some(EntityChannel::Spawning) = self.entity_channels.get(entity) {
            self.entity_channels.remove(entity);

            if self.host_world.contains_key(entity) {
                // initialize component channels
                let mut component_channels = CheckedMap::new();
                let host_components = self.host_world.get(entity).unwrap();
                for component in host_components.iter() {
                    // insert component
                    component_channels.insert(*component, ComponentChannel::Inserting);
                    self.outgoing_actions
                        .send_message(EntityAction::InsertComponent(*entity, *component));
                }
                self.entity_channels
                    .insert(*entity, EntityChannel::Spawned(component_channels));
                self.on_entity_channel_opened(entity);
            } else {
                // despawn entity
                self.entity_channels
                    .insert(*entity, EntityChannel::Despawning);
                self.outgoing_actions
                    .send_message(EntityAction::DespawnEntity(*entity));
                self.on_entity_channel_closing(entity);
            }
        } else {
            panic!("should only receive this event if entity channel is spawning");
        }

        self.remote_world.insert(*entity, CheckedSet::new());
    }

    pub fn remote_despawn_entity(&mut self, entity: &E) {
        if !self.remote_world.contains_key(entity) {
            panic!("should not be able to despawn non-existent entity in remote world");
        }

        if let Some(EntityChannel::Despawning) = self.entity_channels.get(entity) {
            self.entity_channels.remove(entity);
            self.on_entity_channel_closed(entity);

            // if entity is spawned in host, respawn entity channel
            if self.host_world.contains_key(entity) {
                // spawn entity
                self.entity_channels
                    .insert(*entity, EntityChannel::Spawning);
                self.outgoing_actions
                    .send_message(EntityAction::SpawnEntity(*entity));
                self.on_entity_channel_opening(entity);
            }
        } else {
            panic!("should only receive this event if entity channel is despawning");
        }

        self.remote_world.remove(entity);
    }

    pub fn remote_insert_component(&mut self, entity: &E, component: &P::Kind) {
        if !self.remote_world.contains_key(entity) {
            panic!("cannot insert component into non-existent entity");
        }

        let components = self.remote_world.get_mut(entity).unwrap();
        if components.contains(component) {
            panic!("should not be able to replace component in remote world");
        }

        components.insert(*component);

        if let Some(EntityChannel::Spawned(component_channels)) =
            self.entity_channels.get_mut(entity)
        {
            if let Some(ComponentChannel::Inserting) = component_channels.get(component) {
                component_channels.remove(component);

                let host_has_component = self.host_world.get(entity).unwrap().contains(component);
                if host_has_component {
                    // if component exist in host, finalize channel state
                    component_channels.insert(*component, ComponentChannel::Inserted);
                    self.on_component_channel_opened(entity, component);
                } else {
                    // if component doesn't exist in host, start removal
                    component_channels.insert(*component, ComponentChannel::Removing);
                    self.outgoing_actions
                        .send_message(EntityAction::RemoveComponent(*entity, *component));
                    self.on_component_channel_closing(entity, component);
                }
            } else {
                panic!("cannot insert component if component channel has not been initialized");
            }
        } else {
            // entity channel may be despawning, which is okay at this point
            // TODO: enforce this check
        }
    }

    pub fn remote_remove_component(&mut self, entity: &E, component: &P::Kind) {
        if !self.remote_world.contains_key(entity) {
            panic!("cannot remove component from non-existent entity");
        }

        let components = self.remote_world.get_mut(entity).unwrap();
        if !components.contains(component) {
            panic!("should not be able to remove non-existent component in remote world");
        }

        if let Some(EntityChannel::Spawned(component_channels)) =
            self.entity_channels.get_mut(entity)
        {
            if let ComponentChannel::Removing = component_channels.get(component).unwrap() {
                component_channels.remove(component);

                // if component exists in host, start insertion
                let host_has_component = self.host_world.get(entity).unwrap().contains(component);
                if host_has_component {
                    // insert component
                    component_channels.insert(*component, ComponentChannel::Inserting);
                    self.outgoing_actions
                        .send_message(EntityAction::InsertComponent(*entity, *component));
                }
            } else {
                panic!("cannot remove component if component channel has not initiated removal");
            }
        } else {
            // entity channel may be despawning, which is okay at this point
            // TODO: enforce this check
        }

        components.remove(component);
    }

    // State Transition events

    fn on_entity_channel_opening(&mut self, entity: &E) {
        // generate new net entity
        let new_net_entity = self.net_entity_generator.generate();
        self.entity_to_net_entity_map
            .insert(*entity, new_net_entity);
        self.net_entity_to_entity_map
            .insert(new_net_entity, *entity);
    }

    fn on_entity_channel_opened(&mut self, entity: &E) {
        self.delayed_entity_messages.add_entity(entity);
    }

    fn on_entity_channel_closing(&mut self, entity: &E) {
        self.delayed_entity_messages.remove_entity(entity);
    }

    fn on_entity_channel_closed(&mut self, entity: &E) {
        // cleanup net entity
        let net_entity = self.entity_to_net_entity_map.remove(entity).unwrap();
        self.net_entity_to_entity_map.remove(&net_entity);
        self.net_entity_generator.recycle_key(&net_entity);
    }

    fn on_component_channel_opened(&mut self, entity: &E, component: &P::Kind) {
        self.diff_handler
            .register_component(&self.address, entity, component);
    }

    fn on_component_channel_closing(&mut self, entity: &E, component: &P::Kind) {
        self.diff_handler.deregister_component(entity, component);
    }

    // Action Delivery

    pub fn action_delivered(&mut self, action_id: ActionId, action: EntityAction<E, P::Kind>) {
        if self.outgoing_actions.deliver_message(&action_id).is_some() {
            self.delivered_actions.buffer_message(action_id, action);
            self.process_delivered_actions();
        }
    }

    fn process_delivered_actions(&mut self) {
        let delivered_actions = self.delivered_actions.receive_messages();
        for action in delivered_actions {
            match action {
                EntityAction::SpawnEntity(entity) => {
                    self.remote_spawn_entity(&entity);
                }
                EntityAction::DespawnEntity(entity) => {
                    self.remote_despawn_entity(&entity);
                }
                EntityAction::InsertComponent(entity, component) => {
                    self.remote_insert_component(&entity, &component);
                }
                EntityAction::RemoveComponent(entity, component) => {
                    self.remote_remove_component(&entity, &component);
                }
                EntityAction::Noop => {
                    // do nothing
                }
            }
        }
    }

    // Collect

    pub fn take_next_actions(
        &mut self,
        now: &Instant,
        rtt_millis: &f32,
    ) -> VecDeque<(ActionId, EntityAction<E, P::Kind>)> {
        self.outgoing_actions.collect_messages(now, rtt_millis);
        return self.outgoing_actions.take_next_messages();
    }

    pub fn collect_next_updates(&self) -> HashMap<E, HashSet<P::Kind>> {
        let mut output = HashMap::new();

        for (entity, entity_channel) in self.entity_channels.iter() {
            if let EntityChannel::Spawned(component_channels) = entity_channel {
                for (component, component_channel) in component_channels.iter() {
                    if let ComponentChannel::Inserted = component_channel {
                        if self.diff_handler.diff_mask_is_clear(entity, component) {
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

    // Net Entity Conversions

    pub fn entity_to_net_entity(&self, entity: &E) -> Option<&NetEntity> {
        return self.entity_to_net_entity_map.get(entity);
    }

    pub fn net_entity_to_entity(&self, net_entity: &NetEntity) -> Option<&E> {
        return self.net_entity_to_entity_map.get(net_entity);
    }
}

// CheckedMap
pub struct CheckedMap<K: Eq + Hash, V> {
    map: HashMap<K, V>,
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
    set: HashSet<K>,
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

    pub fn iter(&self) -> Iter<'_, K> {
        return self.set.iter();
    }
}
