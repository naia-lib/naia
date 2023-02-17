use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use log::warn;

use naia_shared::{
    ChannelSender, ComponentKind, EntityAction, EntityActionReceiver, Instant, KeyGenerator,
    NetEntity, ReliableSender,
};

use crate::protocol::{
    entity_action_event::EntityActionEvent, entity_manager::ActionId,
    entity_message_waitlist::EntityMessageWaitlist, global_diff_handler::GlobalDiffHandler,
    user_diff_handler::UserDiffHandler,
};

const RESEND_ACTION_RTT_FACTOR: f32 = 1.5;

// ComponentChannel

pub enum ComponentChannel {
    Inserting,
    Inserted,
    Removing,
}

// EntityChannel

pub enum EntityChannel {
    Spawning,
    Spawned(CheckedMap<ComponentKind, ComponentChannel>),
    Despawning,
}

// WorldChannel

/// Channel to perform ECS replication between server and client
/// Only handles entity actions (Spawn/despawn entity and insert/remove components)
/// Will use a reliable sender.
/// Will wait for acks from the client to know the state of the client's ECS world ("remote")
pub struct WorldChannel<E: Copy + Eq + Hash + Send + Sync> {
    /// ECS World that exists currently on the server
    host_world: CheckedMap<E, CheckedSet<ComponentKind>>,
    /// ECS World that exists on the client. Uses packet acks to receive confirmation of the
    /// EntityActions (Entity spawned, component inserted) that were actually received on the client
    remote_world: CheckedMap<E, CheckedSet<ComponentKind>>,
    entity_channels: CheckedMap<E, EntityChannel>,
    outgoing_actions: ReliableSender<EntityActionEvent<E>>,
    delivered_actions: EntityActionReceiver<E>,

    address: SocketAddr,
    pub diff_handler: UserDiffHandler<E>,
    net_entity_generator: KeyGenerator<NetEntity>,
    entity_to_net_entity_map: HashMap<E, NetEntity>,
    net_entity_to_entity_map: HashMap<NetEntity, E>,
    pub delayed_entity_messages: EntityMessageWaitlist<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> WorldChannel<E> {
    pub fn new(address: SocketAddr, diff_handler: &Arc<RwLock<GlobalDiffHandler<E>>>) -> Self {
        Self {
            host_world: CheckedMap::new(),
            remote_world: CheckedMap::new(),
            entity_channels: CheckedMap::new(),
            outgoing_actions: ReliableSender::new(RESEND_ACTION_RTT_FACTOR),
            delivered_actions: EntityActionReceiver::default(),

            address,
            diff_handler: UserDiffHandler::new(diff_handler),
            net_entity_generator: KeyGenerator::default(),
            net_entity_to_entity_map: HashMap::new(),
            entity_to_net_entity_map: HashMap::new(),
            delayed_entity_messages: EntityMessageWaitlist::default(),
        }
    }

    // Main

    pub fn host_has_entity(&self, entity: &E) -> bool {
        self.host_world.contains_key(entity)
    }

    pub fn entity_channel_is_open(&self, entity: &E) -> bool {
        matches!(
            self.entity_channels.get(entity),
            Some(EntityChannel::Spawned(_))
        )
    }

    // Host Updates

    pub fn host_spawn_entity(&mut self, entity: &E) {
        if self.host_world.contains_key(entity) {
            // do nothing
            return;
        }

        self.host_world.insert(*entity, CheckedSet::new());

        if self.entity_channels.get(entity).is_none() {
            // spawn entity
            self.entity_channels
                .insert(*entity, EntityChannel::Spawning);
            self.outgoing_actions
                .send_message(EntityActionEvent::SpawnEntity(*entity));
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
                .send_message(EntityActionEvent::DespawnEntity(*entity));
            self.on_entity_channel_closing(entity);

            for component_kind in removing_components {
                self.on_component_channel_closing(entity, &component_kind);
            }
        }
    }

    pub fn host_insert_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.host_world.contains_key(entity) {
            warn!("World Channel: cannot insert component into non-existent entity");
            return;
        }

        let components = self.host_world.get_mut(entity).unwrap();
        if components.contains(component_kind) {
            // do nothing
            return;
        }

        components.insert(*component_kind);

        if let Some(EntityChannel::Spawned(component_channels)) =
            self.entity_channels.get_mut(entity)
        {
            if component_channels.get(component_kind).is_none() {
                // insert component
                component_channels.insert(*component_kind, ComponentChannel::Inserting);
                self.outgoing_actions
                    .send_message(EntityActionEvent::InsertComponent(*entity, *component_kind));
            }
        }
    }

    pub fn host_remove_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.host_world.contains_key(entity) {
            warn!("World Channel: cannot remove component from non-existent entity");
            return;
        }

        let components = self.host_world.get_mut(entity).unwrap();
        if !components.contains(component_kind) {
            // do nothing
            return;
        }

        components.remove(component_kind);

        if let Some(EntityChannel::Spawned(component_channels)) =
            self.entity_channels.get_mut(entity)
        {
            if let Some(ComponentChannel::Inserted) = component_channels.get(component_kind) {
                component_channels.remove(component_kind);

                // remove component
                component_channels.insert(*component_kind, ComponentChannel::Removing);
                self.outgoing_actions
                    .send_message(EntityActionEvent::RemoveComponent(*entity, *component_kind));
                self.on_component_channel_closing(entity, component_kind);
            }
        }
    }

    // Remote Actions

    pub fn remote_spawn_entity(
        &mut self,
        entity: &E,
        inserted_components: &HashSet<ComponentKind>,
    ) {
        if self.remote_world.contains_key(entity) {
            panic!("World Channel: should not be able to replace entity in remote world");
        }

        if let Some(EntityChannel::Spawning) = self.entity_channels.get(entity) {
            self.remote_world.insert(*entity, CheckedSet::new());
            self.entity_channels.remove(entity);

            if self.host_world.contains_key(entity) {
                // initialize component channels
                let mut component_channels = CheckedMap::new();
                let host_components = self.host_world.get(entity).unwrap();

                let insert_status_components: HashSet<&ComponentKind> =
                    host_components.inner.union(&inserted_components).collect();

                for component in insert_status_components {
                    // change to inserting status
                    component_channels.insert(*component, ComponentChannel::Inserting);
                }

                let send_insert_action_components: HashSet<&ComponentKind> = host_components
                    .inner
                    .difference(&inserted_components)
                    .collect();

                for component in send_insert_action_components {
                    // send insert action
                    self.outgoing_actions
                        .send_message(EntityActionEvent::InsertComponent(*entity, *component));
                }

                self.entity_channels
                    .insert(*entity, EntityChannel::Spawned(component_channels));
                self.on_entity_channel_opened(entity);

                // receive inserted components
                for component in inserted_components {
                    self.remote_insert_component(entity, component);
                }
            } else {
                // despawn entity
                self.entity_channels
                    .insert(*entity, EntityChannel::Despawning);
                self.outgoing_actions
                    .send_message(EntityActionEvent::DespawnEntity(*entity));
                self.on_entity_channel_closing(entity);
            }
        } else {
            panic!("World Channel: should only receive this event if entity channel is spawning");
        }
    }

    pub fn remote_despawn_entity(&mut self, entity: &E) {
        if !self.remote_world.contains_key(entity) {
            panic!(
                "World Channel: should not be able to despawn non-existent entity in remote world"
            );
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
                    .send_message(EntityActionEvent::SpawnEntity(*entity));
                self.on_entity_channel_opening(entity);
            }
        } else {
            panic!("World Channel: should only receive this event if entity channel is despawning");
        }

        self.remote_world.remove(entity);
    }

    pub fn remote_insert_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.remote_world.contains_key(entity) {
            panic!("World Channel: cannot insert component into non-existent entity");
        }

        let components = self.remote_world.get_mut(entity).unwrap();
        if components.contains(component_kind) {
            panic!("World Channel: should not be able to replace component in remote world");
        }

        components.insert(*component_kind);

        if let Some(EntityChannel::Spawned(component_channels)) =
            self.entity_channels.get_mut(entity)
        {
            if let Some(ComponentChannel::Inserting) = component_channels.get(component_kind) {
                component_channels.remove(component_kind);

                let host_has_component = self
                    .host_world
                    .get(entity)
                    .unwrap()
                    .contains(component_kind);
                if host_has_component {
                    // if component exist in host, finalize channel state
                    component_channels.insert(*component_kind, ComponentChannel::Inserted);
                    self.on_component_channel_opened(entity, component_kind);
                } else {
                    // if component doesn't exist in host, start removal
                    component_channels.insert(*component_kind, ComponentChannel::Removing);
                    self.outgoing_actions
                        .send_message(EntityActionEvent::RemoveComponent(*entity, *component_kind));
                    self.on_component_channel_closing(entity, component_kind);
                }
            } else {
                panic!("World Channel: cannot insert component if component channel has not been initialized");
            }
        } else {
            // entity channel may be despawning, which is okay at this point
            // TODO: enforce this check
        }
    }

    pub fn remote_remove_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.remote_world.contains_key(entity) {
            panic!("World Channel: cannot remove component from non-existent entity");
        }

        let components = self.remote_world.get_mut(entity).unwrap();
        if !components.contains(component_kind) {
            panic!("World Channel: should not be able to remove non-existent component in remote world");
        }

        if let Some(EntityChannel::Spawned(component_channels)) =
            self.entity_channels.get_mut(entity)
        {
            if let ComponentChannel::Removing = component_channels.get(component_kind).unwrap() {
                component_channels.remove(component_kind);

                // if component exists in host, start insertion
                let host_has_component = self
                    .host_world
                    .get(entity)
                    .unwrap()
                    .contains(component_kind);
                if host_has_component {
                    // insert component
                    component_channels.insert(*component_kind, ComponentChannel::Inserting);
                    self.outgoing_actions
                        .send_message(EntityActionEvent::InsertComponent(*entity, *component_kind));
                }
            } else {
                panic!("World Channel: cannot remove component if component channel has not initiated removal");
            }
        } else {
            // entity channel may be despawning, which is okay at this point
            // TODO: enforce this check
        }

        components.remove(component_kind);
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

    fn on_component_channel_opened(&mut self, entity: &E, component_kind: &ComponentKind) {
        self.diff_handler
            .register_component(&self.address, entity, component_kind);
    }

    fn on_component_channel_closing(&mut self, entity: &E, component_kind: &ComponentKind) {
        self.diff_handler
            .deregister_component(entity, component_kind);
    }

    // Action Delivery

    pub fn action_delivered(&mut self, action_id: ActionId, action: EntityAction<E>) {
        if self.outgoing_actions.deliver_message(&action_id).is_some() {
            self.delivered_actions.buffer_action(action_id, action);
            self.process_delivered_actions();
        }
    }

    fn process_delivered_actions(&mut self) {
        let delivered_actions = self.delivered_actions.receive_actions();
        for action in delivered_actions {
            match action {
                EntityAction::SpawnEntity(entity, components) => {
                    let component_set: HashSet<ComponentKind> =
                        components.iter().copied().collect();
                    self.remote_spawn_entity(&entity, &component_set);
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
    ) -> VecDeque<(ActionId, EntityActionEvent<E>)> {
        self.outgoing_actions.collect_messages(now, rtt_millis);
        self.outgoing_actions.take_next_messages()
    }

    pub fn collect_next_updates(&self) -> HashMap<E, HashSet<ComponentKind>> {
        let mut output = HashMap::new();

        for (entity, entity_channel) in self.entity_channels.iter() {
            if let EntityChannel::Spawned(component_channels) = entity_channel {
                for (component, component_channel) in component_channels.iter() {
                    if let ComponentChannel::Inserted = component_channel {
                        match self.diff_handler.diff_mask_is_clear(entity, component) {
                            None | Some(true) => {
                                // no updates detected, do nothing
                                continue;
                            }
                            _ => {}
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
        self.entity_to_net_entity_map.get(entity)
    }

    pub fn net_entity_to_entity(&self, net_entity: &NetEntity) -> Option<&E> {
        self.net_entity_to_entity_map.get(net_entity)
    }
}

// CheckedMap
pub struct CheckedMap<K: Eq + Hash, V> {
    pub inner: HashMap<K, V>,
}

impl<K: Eq + Hash, V> CheckedMap<K, V> {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.inner.get_mut(key)
    }

    pub fn insert(&mut self, key: K, value: V) {
        if self.inner.contains_key(&key) {
            panic!("Cannot insert and replace value for given key. Check first.")
        }

        self.inner.insert(key, value);
    }

    pub fn remove(&mut self, key: &K) {
        if !self.inner.contains_key(key) {
            panic!("Cannot remove value for key with non-existent value. Check whether map contains key first.")
        }

        self.inner.remove(key);
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<K, V> {
        self.inner.iter()
    }
}

// CheckedSet
pub struct CheckedSet<K: Eq + Hash> {
    pub inner: HashSet<K>,
}

impl<K: Eq + Hash> CheckedSet<K> {
    pub fn new() -> Self {
        Self {
            inner: HashSet::new(),
        }
    }

    pub fn contains(&self, key: &K) -> bool {
        self.inner.contains(key)
    }

    pub fn insert(&mut self, key: K) {
        if self.inner.contains(&key) {
            panic!("Cannot insert and replace given key. Check first.")
        }

        self.inner.insert(key);
    }

    pub fn remove(&mut self, key: &K) {
        if !self.inner.contains(key) {
            panic!("Cannot remove given non-existent key. Check first.")
        }

        self.inner.remove(key);
    }
}
