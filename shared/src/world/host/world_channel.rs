use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
};

use log::warn;

use super::{
    entity_action_event::EntityActionEvent, host_world_manager::ActionId,
    user_diff_handler::UserDiffHandler,
};
use crate::{
    world::{host::entity_channel::EntityChannel, local_world_manager::LocalWorldManager},
    ChannelSender, ComponentKind, EntityAction, EntityActionReceiver, GlobalWorldManagerType,
    HostEntity, Instant, ReliableSender, WorldRefType,
};

const RESEND_ACTION_RTT_FACTOR: f32 = 1.5;

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

    address: Option<SocketAddr>,
    pub diff_handler: UserDiffHandler<E>,

    outgoing_release_auth_messages: Vec<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> WorldChannel<E> {
    pub fn new(
        address: &Option<SocketAddr>,
        global_world_manager: &dyn GlobalWorldManagerType<E>,
    ) -> Self {
        Self {
            host_world: CheckedMap::new(),
            remote_world: CheckedMap::new(),
            entity_channels: CheckedMap::new(),
            outgoing_actions: ReliableSender::new(RESEND_ACTION_RTT_FACTOR),
            delivered_actions: EntityActionReceiver::new(),

            address: *address,
            diff_handler: UserDiffHandler::new(global_world_manager),

            outgoing_release_auth_messages: Vec::new(),
        }
    }

    // Main

    pub fn host_has_entity(&self, entity: &E) -> bool {
        self.host_world.contains_key(entity)
    }

    pub fn entity_channel_is_open(&self, entity: &E) -> bool {
        if let Some(entity_channel) = self.entity_channels.get(entity) {
            return entity_channel.is_spawned();
        }
        return false;
    }

    pub fn host_component_kinds(&self, entity: &E) -> Vec<ComponentKind> {
        if let Some(component_kinds) = self.host_world.get(entity) {
            component_kinds.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    // returns whether auth release message should be sent
    pub fn entity_release_authority(&mut self, entity: &E) -> bool {
        if let Some(entity_channel) = self.entity_channels.get_mut(entity) {
            let output = entity_channel.release_authority();
            return output;
        } else {
            // request may have not yet come back, that's okay
            return true;
        }
    }

    // Host Updates

    pub fn host_spawn_entity(
        &mut self,
        world_manager: &mut LocalWorldManager<E>,
        entity: &E,
        component_kinds: &Vec<ComponentKind>,
    ) {
        if self.host_world.contains_key(entity) {
            panic!("World Channel: cannot spawn entity that already exists");
        }

        self.host_world.insert(*entity, CheckedSet::new());

        if self.entity_channels.get(entity).is_none() {
            // spawn entity
            self.entity_channels
                .insert(*entity, EntityChannel::new_spawning());
            self.outgoing_actions
                .send_message(EntityActionEvent::SpawnEntity(
                    *entity,
                    component_kinds.clone(),
                ));
            self.on_entity_channel_opening(world_manager, entity);
        }
    }

    pub fn host_despawn_entity(&mut self, entity: &E) {
        if !self.host_world.contains_key(entity) {
            panic!("World Channel: cannot despawn entity that doesn't exist");
        }

        let Some(entity_channel) = self.entity_channels.get_mut(entity) else {
            panic!("World Channel: cannot despawn entity that doesn't have channel")
        };
        if entity_channel.is_spawning() {
            entity_channel.queue_despawn_after_spawned();
            return;
        }
        if entity_channel.is_despawning() {
            panic!("World Channel: cannot despawn entity twice!");
        }

        self.host_world.remove(entity);

        let removing_components = entity_channel.inserted_components();

        entity_channel.despawn();

        self.outgoing_actions
            .send_message(EntityActionEvent::DespawnEntity(*entity));

        for component_kind in removing_components {
            self.on_component_channel_closing(entity, &component_kind);
        }
    }

    pub fn client_initiated_despawn(&mut self, entity: &E) {
        if !self.host_world.contains_key(entity) {
            panic!("World Channel: cannot despawn entity that doesn't exist");
        }

        self.host_world.remove(entity);

        let Some(entity_channel) = self.entity_channels.get(entity) else {
            panic!("World Channel: cannot despawn entity that isn't spawned");
        };
        if !entity_channel.is_spawned() {
            panic!("World Channel: cannot despawn entity that isn't spawned");
        }

        let mut removed_components = Vec::new();

        for component_kind in entity_channel.inserted_components() {
            removed_components.push(component_kind);
        }

        for component_kind in removed_components {
            self.on_component_channel_closing(entity, &component_kind);
        }

        self.entity_channels.remove(entity);
    }

    pub fn host_insert_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.host_world.contains_key(entity) {
            panic!("World Channel: cannot insert component into entity that doesn't exist");
        }

        let components = self.host_world.get_mut(entity).unwrap();
        if components.contains(component_kind) {
            warn!("World Channel: cannot insert component into entity that already has it.. this shouldn't happen?");
            return;
        }

        components.insert(*component_kind);

        if let Some(entity_channel) = self.entity_channels.get_mut(entity) {
            if entity_channel.is_spawned() && !entity_channel.has_component(component_kind) {
                // insert component
                entity_channel.insert_component(component_kind, false);
                self.outgoing_actions
                    .send_message(EntityActionEvent::InsertComponent(*entity, *component_kind));
            }
        }
    }

    pub fn host_remove_component(&mut self, world_entity: &E, component_kind: &ComponentKind) {
        let Some(components) = self.host_world.get_mut(world_entity) else {
            panic!("World Channel: cannot remove component from non-existent entity");
        };
        if !components.contains(component_kind) {
            panic!("World Channel: cannot remove non-existent component from entity");
        }

        components.remove(component_kind);

        if let Some(entity_channel) = self.entity_channels.get_mut(world_entity) {
            if entity_channel.is_spawned() {
                if entity_channel.remove_component(component_kind) {
                    self.outgoing_actions
                        .send_message(EntityActionEvent::RemoveComponent(
                            *world_entity,
                            *component_kind,
                        ));
                    self.on_component_channel_closing(world_entity, component_kind);
                }
            }
        }
    }

    // Track Remote Entities

    pub fn track_remote_entity(
        &mut self,
        local_world_manager: &mut LocalWorldManager<E>,
        entity: &E,
        component_kinds: &Vec<ComponentKind>,
    ) -> HostEntity {
        if self.host_world.contains_key(entity) {
            panic!("World Channel: cannot track remote entity that already exists");
        }

        self.host_world.insert(*entity, CheckedSet::new());
        self.remote_world.insert(*entity, CheckedSet::new());

        // spawn entity
        self.entity_channels
            .insert(*entity, EntityChannel::new_spawned());

        let new_host_entity = self.on_entity_channel_opening(local_world_manager, entity);

        self.delivered_actions
            .track_hosts_redundant_remote_entity(entity, component_kinds);

        new_host_entity
    }

    pub fn untrack_remote_entity(
        &mut self,
        local_world_manager: &mut LocalWorldManager<E>,
        entity: &E,
    ) {
        if !self.host_world.contains_key(entity) {
            panic!("World Channel: cannot untrack remote entity that doesn't exist");
        }

        let components = self.host_world.remove(entity).unwrap();
        for component_kind in components.iter() {
            self.on_component_channel_closing(entity, component_kind);
        }
        self.remote_world.remove(entity);
        self.entity_channels.remove(entity).unwrap();

        local_world_manager.remove_redundant_host_entity(entity);

        self.delivered_actions
            .untrack_hosts_redundant_remote_entity(entity);
    }

    pub fn track_remote_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.host_world.contains_key(entity) {
            panic!("World Channel: cannot insert component into entity that doesn't exist");
        }

        {
            let components = self.remote_world.get_mut(entity).unwrap();
            if components.contains(component_kind) {
                warn!("World Channel: cannot insert component into entity that already has it.. this shouldn't happen?");
                return;
            }

            components.insert(*component_kind);
        }

        {
            let components = self.host_world.get_mut(entity).unwrap();
            if components.contains(component_kind) {
                warn!("World Channel: cannot insert component into entity that already has it.. this shouldn't happen?");
                return;
            }

            components.insert(*component_kind);

            let Some(entity_channel) = self.entity_channels.get_mut(entity) else {
                panic!("Make sure to track remote entity first before calling this method");
            };
            if !entity_channel.is_spawned() {
                panic!("Make sure to track remote entity first before calling this method");
            }
            entity_channel.insert_remote_component(component_kind);
            self.on_component_channel_opened(entity, component_kind);

            // info!("     --- Remote Delegated Entity now is Tracking Component");
        }
    }

    // Remote Actions

    pub fn on_remote_spawn_entity(
        &mut self,
        entity: &E,
        inserted_component_kinds: &HashSet<ComponentKind>,
    ) {
        if self.remote_world.contains_key(entity) {
            panic!("World Channel: should not be able to replace entity in remote world");
        }

        let Some(entity_channel) = self.entity_channels.get_mut(entity) else {
            panic!("World Channel: should only receive this event if entity channel is spawning");
        };
        if !entity_channel.is_spawning() {
            panic!("World Channel: should only receive this event if entity channel is spawning");
        }
        let should_despawn = entity_channel.spawning_complete();

        self.remote_world.insert(*entity, CheckedSet::new());

        if self.host_world.contains_key(entity) {
            // initialize component channels
            let host_components = self.host_world.get(entity).unwrap();

            let inserted_and_inserting_components: HashSet<&ComponentKind> = host_components
                .inner
                .union(&inserted_component_kinds)
                .collect();

            for component_kind in inserted_and_inserting_components {
                // change to inserting status.
                // for the components that have already been inserted, they will be migrated with
                // the `on_remote_insert_component()` call below.
                entity_channel.insert_component(component_kind, true);
            }

            let send_insert_action_component_kinds: HashSet<&ComponentKind> = host_components
                .inner
                .difference(&inserted_component_kinds)
                .collect();

            for component in send_insert_action_component_kinds {
                // send insert action
                self.outgoing_actions
                    .send_message(EntityActionEvent::InsertComponent(*entity, *component));
            }

            // receive inserted components
            for component_kind in inserted_component_kinds {
                self.on_remote_insert_component(entity, component_kind);
            }

            if should_despawn {
                warn!("complete queued despawn");
                self.host_despawn_entity(entity);
            }
        } else {
            // despawn entity
            entity_channel.despawn();

            self.outgoing_actions
                .send_message(EntityActionEvent::DespawnEntity(*entity));
        }
    }

    pub fn on_remote_despawn_entity(
        &mut self,
        local_world_manager: &mut LocalWorldManager<E>,
        entity: &E,
    ) {
        if !self.remote_world.contains_key(entity) {
            panic!(
                "World Channel: should not be able to despawn non-existent entity in remote world"
            );
        }

        let Some(entity_channel) = self.entity_channels.get(entity) else {
            panic!("World Channel: should only receive this event if entity channel is despawning");
        };
        if !entity_channel.is_despawning() {
            panic!("World Channel: should only receive this event if entity channel is despawning");
        }
        self.entity_channels.remove(entity);
        self.on_remote_entity_channel_closed(local_world_manager, entity);

        // if entity is spawned in host, respawn entity channel
        if self.host_world.contains_key(entity) {
            // spawn entity
            self.entity_channels
                .insert(*entity, EntityChannel::new_spawning());
            self.outgoing_actions
                .send_message(EntityActionEvent::SpawnEntity(
                    *entity,
                    self.host_component_kinds(entity),
                ));
            self.on_entity_channel_opening(local_world_manager, entity);
        }

        self.remote_world.remove(entity);
    }

    pub fn on_remote_insert_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.remote_world.contains_key(entity) {
            panic!("World Channel: cannot insert component into non-existent entity");
        }

        let components = self.remote_world.get_mut(entity).unwrap();
        if components.contains(component_kind) {
            panic!("World Channel: should not be able to replace component in remote world");
        }

        components.insert(*component_kind);

        let Some(entity_channel) = self.entity_channels.get_mut(entity) else {
            // entity channel may be despawning, which is okay at this point
            // TODO: enforce this check
            // info!("World Channel: received insert component message for entity without initialized channel, ignoring");
            return;
        };
        if entity_channel.is_despawning() {
            // entity channel may be despawning, which is okay at this point
            // info!("World Channel: received insert component message for despawning entity, ignoring");
            return;
        }
        if !entity_channel.is_spawned() {
            panic!("World Channel: should only receive this event if entity channel is spawned");
        }
        if !entity_channel.component_is_inserting(component_kind) {
            panic!("World Channel: cannot insert component if component channel has not been initialized");
        }
        let host_has_component = self
            .host_world
            .get(entity)
            .unwrap()
            .contains(component_kind);

        let send_entity_auth_release_message =
            entity_channel.component_insertion_complete(component_kind);
        if send_entity_auth_release_message {
            self.outgoing_release_auth_messages.push(*entity);
        }

        if host_has_component {
            // if component exist in host, finalize channel state
            self.on_component_channel_opened(entity, component_kind);
        } else {
            // if component doesn't exist in host, start removal
            entity_channel.remove_component(component_kind);
            self.outgoing_actions
                .send_message(EntityActionEvent::RemoveComponent(*entity, *component_kind));
            self.on_component_channel_closing(entity, component_kind);
        }
    }

    pub fn on_remote_remove_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.remote_world.contains_key(entity) {
            panic!("World Channel: cannot remove component from non-existent entity");
        }

        let components = self.remote_world.get_mut(entity).unwrap();
        if !components.contains(component_kind) {
            panic!("World Channel: should not be able to remove non-existent component in remote world");
        }

        if let Some(entity_channel) = self.entity_channels.get_mut(entity) {
            if !entity_channel.is_spawned() {
                panic!(
                    "World Channel: should only receive this event if entity channel is spawned"
                );
            }
            if !entity_channel.component_is_removing(component_kind) {
                panic!("World Channel: cannot remove component if component channel has not initiated removal");
            }
            let send_auth_release_message =
                entity_channel.component_removal_complete(component_kind);
            if send_auth_release_message {
                self.outgoing_release_auth_messages.push(*entity);
            }

            // if component exists in host, start insertion
            let host_has_component = self
                .host_world
                .get(entity)
                .unwrap()
                .contains(component_kind);
            if host_has_component {
                // insert component
                entity_channel.insert_component(component_kind, false);
                self.outgoing_actions
                    .send_message(EntityActionEvent::InsertComponent(*entity, *component_kind));
            }
        } else {
            // entity channel may be despawning, which is okay at this point
            // TODO: enforce this check
        }

        components.remove(component_kind);
    }

    // State Transition events

    fn on_entity_channel_opening(
        &mut self,
        local_world_manager: &mut LocalWorldManager<E>,
        world_entity: &E,
    ) -> HostEntity {
        if let Some(host_entity) = local_world_manager.remove_reserved_host_entity(world_entity) {
            return host_entity;
        } else {
            let host_entity = local_world_manager.generate_host_entity();
            local_world_manager.insert_host_entity(*world_entity, host_entity);
            return host_entity;
        }
    }

    fn on_remote_entity_channel_closed(
        &mut self,
        local_world_manager: &mut LocalWorldManager<E>,
        entity: &E,
    ) {
        local_world_manager.remove_by_world_entity(entity);
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

    pub fn action_delivered(
        &mut self,
        local_world_manager: &mut LocalWorldManager<E>,
        action_id: ActionId,
        action: EntityAction<E>,
    ) {
        if self.outgoing_actions.deliver_message(&action_id).is_some() {
            self.delivered_actions.buffer_action(action_id, action);
            self.process_delivered_actions(local_world_manager);
        }
    }

    fn process_delivered_actions(&mut self, local_world_manager: &mut LocalWorldManager<E>) {
        let delivered_actions = self.delivered_actions.receive_actions();
        for action in delivered_actions {
            match action {
                EntityAction::SpawnEntity(entity, components) => {
                    let component_set: HashSet<ComponentKind> =
                        components.iter().copied().collect();
                    self.on_remote_spawn_entity(&entity, &component_set);
                }
                EntityAction::DespawnEntity(entity) => {
                    self.on_remote_despawn_entity(local_world_manager, &entity);
                }
                EntityAction::InsertComponent(entity, component_kind) => {
                    self.on_remote_insert_component(&entity, &component_kind);
                }
                EntityAction::RemoveComponent(entity, component) => {
                    self.on_remote_remove_component(&entity, &component);
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

    pub fn collect_next_updates<W: WorldRefType<E>>(
        &self,
        world: &W,
        global_world_manager: &dyn GlobalWorldManagerType<E>,
    ) -> HashMap<E, HashSet<ComponentKind>> {
        let mut output = HashMap::new();

        for (entity, entity_channel) in self.entity_channels.iter() {
            if entity_channel.is_spawned() && world.has_entity(entity) {
                for component_kind in entity_channel.inserted_components() {
                    if global_world_manager.entity_is_replicating(entity)
                        && !self
                            .diff_handler
                            .diff_mask_is_clear(entity, &component_kind)
                        && world.has_component_of_kind(entity, &component_kind)
                    {
                        if !output.contains_key(entity) {
                            output.insert(*entity, HashSet::new());
                        }
                        let send_component_set = output.get_mut(entity).unwrap();
                        send_component_set.insert(component_kind);
                    }
                }
            }
        }
        output
    }

    pub fn collect_auth_release_messages(&mut self) -> Option<Vec<E>> {
        if self.outgoing_release_auth_messages.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut self.outgoing_release_auth_messages))
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

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if !self.inner.contains_key(key) {
            panic!("Cannot remove value for key with non-existent value. Check whether map contains key first.")
        }

        self.inner.remove(key)
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<K, V> {
        self.inner.iter()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
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

    pub fn iter(&self) -> std::collections::hash_set::Iter<K> {
        self.inner.iter()
    }
}
