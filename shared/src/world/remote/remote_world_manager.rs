use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use log::warn;

use naia_socket_shared::Instant;

use crate::{
    world::{
        entity::in_scope_entities::InScopeEntities,
        entity_event::EntityEvent,
        host::host_world_manager::CommandId,
        local::local_entity::RemoteEntity,
        remote::{
            remote_entity_waitlist::{RemoteEntityWaitlist, WaitlistStore},
            remote_world_waitlist::RemoteWorldWaitlist,
        },
        sync::{RemoteEngine, RemoteEntityChannel},
    },
    ComponentKind, ComponentKinds, EntityAndGlobalEntityConverter, EntityAuthStatus, EntityCommand,
    EntityMessage, EntityMessageReceiver, GlobalEntity, GlobalEntitySpawner,
    GlobalWorldManagerType, HostType, LocalEntityAndGlobalEntityConverter, LocalEntityMap,
    MessageIndex, OwnedLocalEntity, PendingComponentUpdate, Replicate, Tick, WorldMutType,
};

cfg_if! {
    if #[cfg(feature = "e2e_debug")] {
        use crate::world::{
            host::host_world_manager::SubCommandId,
            sync::remote_entity_channel::EntityChannelState,
        };
        use crate::EntityMessageType;
    }
}

/// Manages the inbound side of entity replication — entities whose authoritative state comes from the remote peer.
pub struct RemoteWorldManager {
    // For Server, this contains the Entities that have been received from the Client, that the Client has authority over.
    // For Client, this contains the Entities that have been received from the Server, that the Server has authority over.
    remote_engine: RemoteEngine<RemoteEntity>,

    // For Server, this is None
    // For Client, it reflects the delegated RemoteEntities it has temporary authority over
    authed_entities_opt: Option<HashSet<RemoteEntity>>,

    // incoming messages
    incoming_events: Vec<EntityEvent>,
    waitlist: RemoteWorldWaitlist,
    // outgoing messages
}

impl RemoteWorldManager {
    /// Creates a `RemoteWorldManager` for the given `host_type` side of a connection.
    pub fn new(host_type: HostType) -> Self {
        let delegated_world_opt = if host_type == HostType::Client {
            Some(HashSet::new())
        } else {
            None
        };
        Self {
            remote_engine: RemoteEngine::new(host_type),
            authed_entities_opt: delegated_world_opt,
            incoming_events: Vec::new(),
            waitlist: RemoteWorldWaitlist::new(),
        }
    }

    pub(crate) fn deliver_message(
        &mut self,
        _command_id: CommandId,
        _message: EntityMessage<RemoteEntity>,
    ) {
        // so far, it seems like we don't need to do anything specific when delivering a remote-entity message.. we'll see
    }

    pub(crate) fn entity_waitlist_queue<T>(
        &mut self,
        remote_entity_set: &HashSet<RemoteEntity>,
        waitlist_store: &mut WaitlistStore<T>,
        message: T,
    ) {
        self.waitlist.entity_waitlist_mut().queue(
            &self.remote_engine,
            remote_entity_set,
            waitlist_store,
            message,
        );
    }

    /// Returns a shared reference to the entity waitlist.
    pub fn entity_waitlist(&self) -> &RemoteEntityWaitlist {
        self.waitlist.entity_waitlist()
    }

    /// Returns a mutable reference to the entity waitlist.
    pub fn entity_waitlist_mut(&mut self) -> &mut RemoteEntityWaitlist {
        self.waitlist.entity_waitlist_mut()
    }

    pub(crate) fn register_authed_entity(&mut self, remote_entity: &RemoteEntity) {
        let Some(authed_entities) = self.authed_entities_opt.as_mut() else {
            return;
        };

        authed_entities.insert(*remote_entity);
    }

    #[cfg(feature = "e2e_debug")]
    pub fn debug_channel_diagnostic(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Option<(
        EntityChannelState,
        (SubCommandId, usize, Option<SubCommandId>, usize),
    )> {
        self.remote_engine
            .get_world()
            .get(remote_entity)
            .map(|channel| channel.debug_auth_diagnostic())
    }

    #[cfg(feature = "e2e_debug")]
    pub fn debug_channel_snapshot(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Option<(
        EntityChannelState,
        Option<MessageIndex>,
        usize,
        Option<(MessageIndex, EntityMessageType)>,
        Option<MessageIndex>,
    )> {
        self.remote_engine
            .get_world()
            .get(remote_entity)
            .map(|channel| channel.debug_channel_snapshot())
    }

    pub(crate) fn deregister_authed_entity(&mut self, remote_entity: &RemoteEntity) {
        let Some(authed_entities) = self.authed_entities_opt.as_mut() else {
            return;
        };

        authed_entities.remove(remote_entity);
    }

    pub(crate) fn is_component_updatable(
        &self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        global_entity: &GlobalEntity,
        kind: &ComponentKind,
    ) -> bool {
        let Some(authed_entities) = self.authed_entities_opt.as_ref() else {
            return false;
        };
        let Ok(remote_entity) = local_converter.global_entity_to_remote_entity(global_entity)
        else {
            return false;
        };
        if !authed_entities.contains(&remote_entity) {
            return false;
        }
        let Some(remote_channel) = self.remote_engine.get_world().get(&remote_entity) else {
            return false;
        };
        remote_channel.has_component_kind(kind)
    }

    /// Drains and returns all pending outbound [`EntityCommand`]s from the remote engine.
    pub fn take_outgoing_commands(&mut self) -> Vec<EntityCommand> {
        self.remote_engine.take_outgoing_commands()
    }

    /// Enqueues `command` for the entity identified in `command` via the remote engine, silently skipping if the entity no longer exists.
    pub fn send_entity_command(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        command: EntityCommand,
    ) {
        let global_entity = command.entity();
        // Entity may no longer exist if it went out of scope before this command
        // was processed. In that case, the command is no longer relevant - silently skip.
        let Ok(remote_entity) = converter.global_entity_to_remote_entity(&global_entity) else {
            warn!(
                "send_entity_command: entity {:?} no longer exists (likely out of scope), skipping",
                global_entity
            );
            return;
        };
        self.remote_engine
            .send_entity_command(remote_entity, command);
    }

    pub(crate) fn send_auth_command(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        command: EntityCommand,
    ) {
        let global_entity = command.entity();
        // Entity may no longer exist if it went out of scope before this auth command
        // was processed. In that case, the command is no longer relevant - silently skip.
        let Ok(remote_entity) = converter.global_entity_to_remote_entity(&global_entity) else {
            warn!(
                "send_auth_command: entity {:?} no longer exists (likely out of scope), skipping",
                global_entity
            );
            return;
        };
        self.remote_engine.send_auth_command(remote_entity, command);
    }

    /// Update authority status in RemoteEntityChannel (used after migration)
    pub(crate) fn receive_set_auth_status(
        &mut self,
        remote_entity: RemoteEntity,
        auth_status: EntityAuthStatus,
    ) {
        self.remote_engine
            .receive_set_auth_status(remote_entity, auth_status);
    }

    /// Notifies the waitlist that `entity` has been spawned, unblocking any queued operations.
    pub fn spawn_entity(
        &mut self,
        // converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &RemoteEntity,
    ) {
        self.waitlist.spawn_entity(&self.remote_engine, entity);
    }

    /// Removes `entity` from the waitlist tracking structures.
    pub fn despawn_entity(
        &mut self,
        _local_entity_map: &mut LocalEntityMap,
        entity: &RemoteEntity,
    ) {
        self.waitlist.despawn_entity(entity);
    }

    /// Processes all buffered incoming messages and updates, applying them to `world` and returning the resulting [`EntityEvent`]s.
    #[allow(clippy::too_many_arguments)]
    pub fn take_incoming_events<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        spawner: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        local_entity_map: &mut LocalEntityMap,
        component_kinds: &ComponentKinds,
        world: &mut W,
        now: &Instant,
        incoming_components: &mut HashMap<(OwnedLocalEntity, ComponentKind), Box<dyn Replicate>>,
        incoming_updates: Vec<(Tick, OwnedLocalEntity, PendingComponentUpdate)>,
        incoming_messages: Vec<(MessageIndex, EntityMessage<RemoteEntity>)>,
    ) -> Vec<EntityEvent> {
        let incoming_messages = EntityMessageReceiver::remote_take_incoming_messages(
            &mut self.remote_engine,
            incoming_messages,
        );

        self.process_updates(
            local_entity_map.entity_converter(),
            spawner.to_converter(),
            component_kinds,
            world,
            now,
            incoming_updates,
        );
        self.process_incoming_messages(
            spawner,
            global_world_manager,
            local_entity_map,
            world,
            now,
            incoming_components,
            incoming_messages,
        );

        std::mem::take(&mut self.incoming_events)
    }

    #[allow(clippy::too_many_arguments)]
    fn process_incoming_messages<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        spawner: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        local_entity_map: &mut LocalEntityMap,
        world: &mut W,
        now: &Instant,
        incoming_components: &mut HashMap<(OwnedLocalEntity, ComponentKind), Box<dyn Replicate>>,
        incoming_messages: Vec<EntityMessage<RemoteEntity>>,
    ) {
        self.process_ready_messages(
            spawner,
            global_world_manager,
            local_entity_map,
            world,
            incoming_components,
            incoming_messages,
        );
        let world_converter = spawner.to_converter();
        self.process_waitlist_messages(
            local_entity_map.entity_converter(),
            world_converter,
            world,
            now,
        );
    }

    /// For each [`EntityMessage`] that can be executed now,
    /// execute it and emit a corresponding event.
    fn process_ready_messages<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        spawner: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        local_entity_map: &mut LocalEntityMap,
        world: &mut W,
        incoming_components: &mut HashMap<(OwnedLocalEntity, ComponentKind), Box<dyn Replicate>>,
        incoming_messages: Vec<EntityMessage<RemoteEntity>>,
    ) {
        // execute the action and emit an event
        for message in incoming_messages {
            // info!("Processing EntityMessage: {:?}", message);
            match message {
                EntityMessage::Spawn(remote_entity) => {
                    // set up entity
                    let world_entity = world.spawn_entity();
                    let global_entity = spawner.spawn(world_entity, Some(remote_entity));
                    let already_mapped = local_entity_map.contains_remote_entity(&remote_entity);
                    if already_mapped {
                        // mapped remote entity already when reserving global entity
                    } else {
                        local_entity_map.insert_with_remote_entity(global_entity, remote_entity);
                    }

                    self.incoming_events.push(EntityEvent::Spawn(global_entity));
                }
                EntityMessage::Despawn(remote_entity) => {
                    // On the client (authed_entities_opt is Some): read the mapping before
                    // removing it so that process_remove can resolve remote_entity →
                    // global_entity and emit RemoveComponent events. The client has no relay
                    // path, so firing EntityEvent::RemoveComponent here is necessary
                    // for the events API to surface component removals on entity despawn.
                    //
                    // On the server (authed_entities_opt is None): remove the mapping first.
                    // process_remove then silently skips event emission (converter lookup
                    // fails) because the server already fires push_remove_synthetic in
                    // world_server.rs before the despawn — firing it again here would cause
                    // remove_component_worldless to be called for an entity whose component
                    // records have already been cleaned up.
                    let is_client = self.authed_entities_opt.is_some();
                    let global_entity = if is_client {
                        *local_entity_map
                            .global_entity_from_remote(&remote_entity)
                            .unwrap()
                    } else {
                        local_entity_map.remove_by_remote_entity(&remote_entity)
                    };
                    let world_entity = spawner.global_entity_to_entity(&global_entity).unwrap();

                    if let Some(component_kinds) =
                        global_world_manager.component_kinds(&global_entity)
                    {
                        for component_kind in component_kinds {
                            self.process_remove(
                                world,
                                local_entity_map,
                                &remote_entity,
                                &world_entity,
                                &component_kind,
                            );
                        }
                    }

                    if is_client {
                        local_entity_map.remove_by_remote_entity(&remote_entity);
                    }
                    world.despawn_entity(&world_entity);

                    self.incoming_events
                        .push(EntityEvent::Despawn(global_entity));
                }
                EntityMessage::InsertComponent(remote_entity, component_kind) => {
                    let local_entity = remote_entity.copy_to_owned();
                    let component = incoming_components
                        .remove(&(local_entity, component_kind))
                        .unwrap();

                    if local_entity_map.contains_remote_entity(&remote_entity) {
                        let global_entity = *local_entity_map
                            .global_entity_from_remote(&remote_entity)
                            .unwrap();
                        let world_entity = spawner.global_entity_to_entity(&global_entity).unwrap();

                        self.process_insert(
                            world,
                            local_entity_map,
                            &remote_entity,
                            &world_entity,
                            component,
                            &component_kind,
                        );
                    } else {
                        // entity may have despawned on disconnect or something similar?
                        warn!("received InsertComponent message for nonexistant entity");
                    }
                }
                EntityMessage::RemoveComponent(remote_entity, component_kind) => {
                    let global_entity = local_entity_map
                        .global_entity_from_remote(&remote_entity)
                        .unwrap();
                    let world_entity = spawner.global_entity_to_entity(global_entity).unwrap();
                    self.process_remove(
                        world,
                        local_entity_map,
                        &remote_entity,
                        &world_entity,
                        &component_kind,
                    );
                }
                EntityMessage::Noop => {
                    // do nothing
                }
                EntityMessage::SetAuthority(_, remote_entity, auth_status) => {
                    // Update the stored auth status so get_entity_auth_status() reflects the new value
                    self.remote_engine
                        .receive_set_auth_status(remote_entity, auth_status);
                    let Some(global_entity) =
                        local_entity_map.global_entity_from_remote(&remote_entity)
                    else {
                        continue;
                    };
                    self.incoming_events
                        .push(EntityEvent::SetAuthority(*global_entity, auth_status));
                }
                msg => {
                    // let msg_type = msg.get_type();
                    let event = msg.to_event(local_entity_map);
                    self.incoming_events.push(event);
                }
            }
        }
    }

    fn process_insert<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &RemoteEntity,
        world_entity: &E,
        component: Box<dyn Replicate>,
        component_kind: &ComponentKind,
    ) {
        if let Some(remote_entity_set) = component.relations_waiting() {
            self.waitlist.waitlist_queue_entity(
                &self.remote_engine,
                entity,
                component,
                component_kind,
                &remote_entity_set,
            );
        } else {
            self.finish_insert(
                world,
                converter,
                entity,
                world_entity,
                component,
                component_kind,
            );
        }
    }

    fn finish_insert<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &RemoteEntity,
        world_entity: &E,
        component: Box<dyn Replicate>,
        component_kind: &ComponentKind,
    ) {
        // let name = component.name();
        // info!(
        //     "Remote World Manager: finish inserting component {:?} for entity {:?}",
        //     &name, global_entity
        // );

        world.insert_boxed_component(world_entity, component);

        let global_entity = converter.remote_entity_to_global_entity(entity).unwrap();

        self.incoming_events
            .push(EntityEvent::InsertComponent(global_entity, *component_kind));
    }

    fn process_remove<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &RemoteEntity,
        world_entity: &E,
        component_kind: &ComponentKind,
    ) {
        if self.waitlist.process_remove(entity, component_kind) {
            return;
        }
        // Remove from world
        if let Some(component) = world.remove_component_of_kind(world_entity, component_kind) {
            // Send out event
            if let Ok(global_entity) = converter.remote_entity_to_global_entity(entity) {
                self.incoming_events
                    .push(EntityEvent::RemoveComponent(global_entity, component));
            }
        }
    }

    fn process_waitlist_messages<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        world: &mut W,
        now: &Instant,
    ) {
        for (entity, component_kind, component) in
            self.waitlist.entities_to_insert(now, local_converter)
        {
            // The target entity may have despawned while the component sat
            // in the waitlist (e.g. an avatar's despawn racing a waitlisted
            // AssetRef insert) — `RemoteWorldWaitlist::despawn_entity` does
            // not purge queued inserts, so a ready item can reference a
            // dead entity. The insert is moot then: drop it.
            let Ok(global_entity) = local_converter.remote_entity_to_global_entity(&entity) else {
                continue;
            };
            let Ok(world_entity) = world_converter.global_entity_to_entity(&global_entity) else {
                continue;
            };
            self.finish_insert(
                world,
                local_converter,
                &entity,
                &world_entity,
                component,
                &component_kind,
            );
        }
    }

    fn process_updates<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        now: &Instant,
        incoming_updates: Vec<(Tick, OwnedLocalEntity, PendingComponentUpdate)>,
    ) {
        self.process_ready_updates(
            local_converter,
            world_converter,
            component_kinds,
            world,
            incoming_updates,
        );
        self.process_waitlist_updates(local_converter, world_converter, world, now);
        self.process_self_waitlist_updates(local_converter, world_converter, world, now);
    }

    /// Process component updates from raw bits for a given entity
    fn process_ready_updates<WE: Copy + Eq + Hash + Send + Sync, W: WorldMutType<WE>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<WE>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        incoming_updates: Vec<(Tick, OwnedLocalEntity, PendingComponentUpdate)>,
    ) {
        for (tick, local_entity, component_kind) in self.waitlist.process_ready_updates(
            &self.remote_engine,
            local_converter,
            world_converter,
            component_kinds,
            world,
            incoming_updates,
        ) {
            let global_entity = local_converter
                .owned_entity_to_global_entity(&local_entity)
                .unwrap();
            self.incoming_events.push(EntityEvent::UpdateComponent(
                tick,
                global_entity,
                component_kind,
            ));
        }
    }

    fn process_waitlist_updates<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        world: &mut W,
        now: &Instant,
    ) {
        for (tick, remote_entity, component_kind) in
            self.waitlist
                .process_waitlist_updates(local_converter, world_converter, world, now)
        {
            let global_entity = local_converter
                .remote_entity_to_global_entity(&remote_entity)
                .unwrap();
            self.incoming_events.push(EntityEvent::UpdateComponent(
                tick,
                global_entity,
                component_kind,
            ));
        }
    }

    /// Emit `UpdateComponent` events for updates that were buffered waiting on
    /// their own target entity to spawn, and have now been applied. Mirrors
    /// `process_waitlist_updates`.
    fn process_self_waitlist_updates<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        world: &mut W,
        now: &Instant,
    ) {
        for (tick, remote_entity, component_kind) in self.waitlist.process_self_waitlist_updates(
            local_converter,
            world_converter,
            world,
            now,
        ) {
            let global_entity = local_converter
                .remote_entity_to_global_entity(&remote_entity)
                .unwrap();
            self.incoming_events.push(EntityEvent::UpdateComponent(
                tick,
                global_entity,
                component_kind,
            ));
        }
    }

    pub(crate) fn force_drain_entity_buffers(&mut self, remote_entity: &RemoteEntity) {
        let Some(channel) = self.remote_engine.get_world_mut().get_mut(remote_entity) else {
            panic!("Cannot force-drain non-existent entity");
        };
        channel.force_drain_all_buffers();
    }

    pub(crate) fn extract_component_kinds(
        &self,
        remote_entity: &RemoteEntity,
    ) -> HashSet<ComponentKind> {
        let Some(channel) = self.remote_engine.get_world().get(remote_entity) else {
            panic!("Cannot extract component kinds from non-existent entity");
        };
        channel.extract_inserted_component_kinds()
    }

    pub(crate) fn remove_entity_channel(
        &mut self,
        remote_entity: &RemoteEntity,
    ) -> RemoteEntityChannel {
        self.remote_engine.remove_entity_channel(remote_entity)
    }

    pub(crate) fn insert_entity_channel(
        &mut self,
        remote_entity: RemoteEntity,
        channel: RemoteEntityChannel,
    ) {
        self.remote_engine
            .insert_entity_channel(remote_entity, channel);
    }

    pub(crate) fn has_entity_channel(&self, remote_entity: &RemoteEntity) -> bool {
        self.remote_engine.has_entity(remote_entity)
    }

    pub(crate) fn get_entity_channel_mut(
        &mut self,
        remote_entity: &RemoteEntity,
    ) -> Option<&mut RemoteEntityChannel> {
        self.remote_engine.get_entity_channel_mut(remote_entity)
    }

    /// See [`RemoteEngine::flush_entity_channel`] — surfaces messages a
    /// migration upgrade released from the channel's pre-spawn buffers.
    pub(crate) fn flush_entity_channel(&mut self, remote_entity: RemoteEntity) {
        self.remote_engine.flush_entity_channel(remote_entity);
    }

    /// Returns the current authority status for `entity`'s remote channel, if one exists.
    pub fn get_entity_auth_status(&self, entity: &RemoteEntity) -> Option<EntityAuthStatus> {
        self.remote_engine.get_entity_auth_status(entity)
    }

    /// Queues `command` directly onto the remote engine's outgoing command buffer for reliable
    /// transmission to the server.  Only call this for intentional client-initiated despawns of
    /// server-created entities where the client holds Granted authority.
    pub fn push_outgoing_despawn(&mut self, command: EntityCommand) {
        self.remote_engine.push_outgoing_despawn(command);
    }
}

impl InScopeEntities<RemoteEntity> for RemoteWorldManager {
    fn has_entity(&self, entity: &RemoteEntity) -> bool {
        self.remote_engine.has_entity(entity)
    }
}

#[cfg(test)]
mod remote_world_manager_tests {
    use super::*;

    use crate::{
        bigmap::BigMapKey,
        world::entity::error::EntityDoesNotExistError,
        world::{
            test_support::TestGwm,
            test_world::{remote_component, TestSpawner, TestWorld},
        },
        EntityMessageType, Property,
    };

    #[derive(Replicate)]
    struct Ghost {
        value: Property<u8>,
    }

    /// A second kind, so "the channel does not carry this component" can be
    /// asked without inventing an entity for it.
    #[derive(Replicate)]
    struct Wraith {
        value: Property<u8>,
    }

    fn kinds() -> ComponentKinds {
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();
        kinds
    }

    fn ghost() -> ComponentKind {
        ComponentKind::of::<Ghost>()
    }

    fn remote(id: u16) -> RemoteEntity {
        RemoteEntity::new(id as u32)
    }

    /// Everything `take_incoming_events` needs, plus the message index counter
    /// the remote engine orders by.
    struct Fixture {
        manager: RemoteWorldManager,
        spawner: TestSpawner,
        gwm: TestGwm,
        map: LocalEntityMap,
        kinds: ComponentKinds,
        world: TestWorld,
        next_index: MessageIndex,
    }

    impl Fixture {
        fn new(host_type: HostType) -> Self {
            let kinds = kinds();
            Self {
                manager: RemoteWorldManager::new(host_type),
                spawner: TestSpawner::new(),
                gwm: TestGwm::new(&kinds),
                map: LocalEntityMap::new(host_type),
                kinds,
                world: TestWorld::new(),
                next_index: 0,
            }
        }

        /// Feeds `messages` through the manager and returns the events raised.
        fn deliver(&mut self, messages: Vec<EntityMessage<RemoteEntity>>) -> Vec<EntityEvent> {
            self.deliver_with(messages, &mut HashMap::new())
        }

        fn deliver_with(
            &mut self,
            messages: Vec<EntityMessage<RemoteEntity>>,
            incoming_components: &mut HashMap<
                (OwnedLocalEntity, ComponentKind),
                Box<dyn Replicate>,
            >,
        ) -> Vec<EntityEvent> {
            let indexed = messages
                .into_iter()
                .map(|message| {
                    let index = self.next_index;
                    self.next_index += 1;
                    (index, message)
                })
                .collect();
            self.manager.take_incoming_events(
                &mut self.spawner,
                &self.gwm,
                &mut self.map,
                &self.kinds,
                &mut self.world,
                &Instant::now(),
                incoming_components,
                Vec::new(),
                indexed,
            )
        }

        /// Feeds `updates` through the manager with no messages.
        fn deliver_updates(
            &mut self,
            updates: Vec<(Tick, OwnedLocalEntity, PendingComponentUpdate)>,
        ) -> Vec<EntityEvent> {
            self.manager.take_incoming_events(
                &mut self.spawner,
                &self.gwm,
                &mut self.map,
                &self.kinds,
                &mut self.world,
                &Instant::now(),
                &mut HashMap::new(),
                updates,
                Vec::new(),
            )
        }

        /// Spawns `entity` remotely and gives it a Ghost, which is the state
        /// every later message assumes.
        fn spawn_with_ghost(&mut self, entity: RemoteEntity) -> GlobalEntity {
            let mut components: HashMap<(OwnedLocalEntity, ComponentKind), Box<dyn Replicate>> =
                HashMap::new();
            components.insert(
                (entity.copy_to_owned(), ghost()),
                remote_component(&self.kinds, &Ghost::new_complete(1)),
            );
            self.deliver_with(
                vec![
                    EntityMessage::Spawn(entity),
                    EntityMessage::InsertComponent(entity, ghost()),
                ],
                &mut components,
            );
            let global = *self
                .map
                .global_entity_from_remote(&entity)
                .expect("the spawn must have mapped the entity");
            self.gwm.declare_kinds(&global, vec![ghost()]);
            global
        }
    }

    /// `EntityEvent` carries a boxed component and so is neither `Debug` nor
    /// `PartialEq`. Its type and subject are what the tests are about.
    fn summarize(events: &[EntityEvent]) -> Vec<(Option<EntityMessageType>, GlobalEntity)> {
        events
            .iter()
            .map(|event| (event.to_type(), event.entity()))
            .collect()
    }

    #[test]
    fn a_spawn_message_maps_the_entity_and_raises_a_spawn_event() {
        let mut fixture = Fixture::new(HostType::Client);
        let events = fixture.deliver(vec![EntityMessage::Spawn(remote(1))]);

        let global = *fixture
            .map
            .global_entity_from_remote(&remote(1))
            .expect("a spawned entity must be mapped");
        assert_eq!(
            summarize(&events),
            vec![(Some(EntityMessageType::Spawn), global)],
        );
    }

    #[test]
    fn inserting_a_component_puts_it_in_the_world_and_raises_an_event() {
        let mut fixture = Fixture::new(HostType::Client);
        let global = fixture.spawn_with_ghost(remote(1));

        assert_eq!(
            *fixture
                .world
                .value_of::<Ghost>(&BigMapKey::to_u64(&global))
                .expect("the component must have reached the world")
                .value,
            1,
        );
    }

    #[test]
    fn a_component_for_an_entity_that_was_never_spawned_is_dropped() {
        let mut fixture = Fixture::new(HostType::Client);
        let mut components: HashMap<(OwnedLocalEntity, ComponentKind), Box<dyn Replicate>> =
            HashMap::new();
        components.insert(
            (remote(1).copy_to_owned(), ghost()),
            remote_component(&kinds(), &Ghost::new_complete(1)),
        );

        let events = fixture.deliver_with(
            vec![EntityMessage::InsertComponent(remote(1), ghost())],
            &mut components,
        );

        assert!(
            summarize(&events).is_empty(),
            "a component for an unknown entity must not raise an insert event",
        );
    }

    #[test]
    fn removing_a_component_raises_an_event_carrying_the_value_it_had() {
        let mut fixture = Fixture::new(HostType::Client);
        let global = fixture.spawn_with_ghost(remote(1));

        let events = fixture.deliver(vec![EntityMessage::RemoveComponent(remote(1), ghost())]);

        assert_eq!(
            summarize(&events),
            vec![(Some(EntityMessageType::RemoveComponent), global)],
        );
        let EntityEvent::RemoveComponent(_, component) = &events[0] else {
            panic!("expected a RemoveComponent event");
        };
        assert_eq!(component.kind(), ghost());
        assert!(
            fixture
                .world
                .value_of::<Ghost>(&BigMapKey::to_u64(&global))
                .is_none(),
            "the component must be gone from the world, not just reported",
        );
    }

    /// The client fires RemoveComponent for each of a despawned entity's
    /// components; the server does not, because `world_server.rs` has already
    /// fired them synthetically before the despawn reaches here. The mapping
    /// removal order is what makes the difference: the client reads the
    /// mapping first and drops it afterwards, the server drops it up front so
    /// the converter lookup in `process_remove` fails and the event is
    /// skipped.
    #[test]
    fn a_client_reports_the_components_a_despawn_takes_with_it() {
        let mut fixture = Fixture::new(HostType::Client);
        let global = fixture.spawn_with_ghost(remote(1));

        let events = fixture.deliver(vec![EntityMessage::Despawn(remote(1))]);

        assert_eq!(
            summarize(&events),
            vec![
                (Some(EntityMessageType::RemoveComponent), global),
                (Some(EntityMessageType::Despawn), global),
            ],
        );
        assert!(
            !fixture.map.contains_remote_entity(&remote(1)),
            "the mapping must be dropped once the despawn is done with it",
        );
    }

    #[test]
    fn a_server_reports_only_the_despawn_because_the_removals_already_fired() {
        let mut fixture = Fixture::new(HostType::Server);
        let global = fixture.spawn_with_ghost(remote(1));

        let events = fixture.deliver(vec![EntityMessage::Despawn(remote(1))]);

        assert_eq!(
            summarize(&events),
            vec![(Some(EntityMessageType::Despawn), global)],
            "firing removals here would double-remove records world_server \
             has already cleaned up",
        );
        assert!(!fixture.map.contains_remote_entity(&remote(1)));
    }

    #[test]
    fn a_noop_message_raises_nothing() {
        let mut fixture = Fixture::new(HostType::Client);
        let events = fixture.deliver(vec![EntityMessage::Noop]);
        assert!(summarize(&events).is_empty());
    }

    /// Everything the manager does not handle specially is turned into an
    /// event by `EntityMessage::to_event`, which resolves the entity through
    /// the map. The message still has to survive the entity channel's auth
    /// gate to get here, so this uses the two that a spawned, undelegated
    /// entity accepts.
    #[test]
    fn a_message_with_no_special_handling_becomes_its_own_event() {
        for (message, expected) in [
            (
                EntityMessage::EnableDelegation(0, remote(1)),
                EntityMessageType::EnableDelegation,
            ),
            (
                EntityMessage::Unpublish(0, remote(1)),
                EntityMessageType::Unpublish,
            ),
        ] {
            let mut fixture = Fixture::new(HostType::Client);
            let global = fixture.spawn_with_ghost(remote(1));

            let events = fixture.deliver(vec![message]);

            assert_eq!(summarize(&events), vec![(Some(expected), global)]);
        }
    }

    // -- what a delegated client may update ---------------------------------

    /// `is_component_updatable` gates whether a client may write to a
    /// component it does not own. Every one of its five refusals returns the
    /// same `false`, so each has to be reached on its own.
    #[test]
    fn only_an_authed_entitys_own_components_are_updatable() {
        let mut fixture = Fixture::new(HostType::Client);
        let global = fixture.spawn_with_ghost(remote(1));
        let converter_map = {
            let mut map = LocalEntityMap::new(HostType::Client);
            map.insert_with_remote_entity(global, remote(1));
            map
        };

        assert!(
            !fixture.manager.is_component_updatable(
                converter_map.entity_converter(),
                &global,
                &ghost(),
            ),
            "an entity this peer has no authority over is not updatable",
        );

        fixture.manager.register_authed_entity(&remote(1));
        assert!(
            fixture.manager.is_component_updatable(
                converter_map.entity_converter(),
                &global,
                &ghost(),
            ),
            "an authed entity's own component is updatable",
        );
        assert!(
            !fixture.manager.is_component_updatable(
                converter_map.entity_converter(),
                &global,
                &ComponentKind::of::<Wraith>(),
            ),
            "a component the channel does not carry is not updatable",
        );

        let unmapped = LocalEntityMap::new(HostType::Client);
        assert!(
            !fixture
                .manager
                .is_component_updatable(unmapped.entity_converter(), &global, &ghost()),
            "an entity with no remote address cannot be checked at all",
        );

        fixture.manager.deregister_authed_entity(&remote(1));
        assert!(
            !fixture.manager.is_component_updatable(
                converter_map.entity_converter(),
                &global,
                &ghost(),
            ),
            "authority handed back must close the gate again",
        );
    }

    #[test]
    fn a_server_never_treats_a_component_as_remotely_updatable() {
        let mut fixture = Fixture::new(HostType::Server);
        let global = fixture.spawn_with_ghost(remote(1));
        let mut map = LocalEntityMap::new(HostType::Server);
        map.insert_with_remote_entity(global, remote(1));

        // The server has no authed set at all, so registering is a no-op
        // rather than an error.
        fixture.manager.register_authed_entity(&remote(1));

        assert!(
            !fixture
                .manager
                .is_component_updatable(map.entity_converter(), &global, &ghost()),
            "only a client holds delegated authority over a remote entity",
        );
    }

    // -- commands going back out --------------------------------------------

    #[test]
    fn a_command_for_an_entity_that_left_scope_is_dropped_rather_than_sent() {
        let mut fixture = Fixture::new(HostType::Client);
        let global = fixture.spawn_with_ghost(remote(1));
        let empty = LocalEntityMap::new(HostType::Client);

        fixture
            .manager
            .send_entity_command(empty.entity_converter(), EntityCommand::Despawn(global));
        fixture.manager.send_auth_command(
            empty.entity_converter(),
            EntityCommand::Publish(None, global),
        );

        assert!(
            fixture.manager.take_outgoing_commands().is_empty(),
            "a command naming an entity with no remote address has nowhere to go",
        );
    }

    // -- things that arrive before what they depend on ----------------------

    use crate::EntityProperty;

    /// A component that points at another entity.
    #[derive(Replicate)]
    struct Haunt {
        value: Property<u8>,
        target: EntityProperty,
    }

    /// Addresses every entity as `referenced`'s HOST half, which reverses on
    /// the way out so the component lands on the wire naming a remote entity
    /// the receiver may not have yet.
    struct PointsAt {
        referenced: RemoteEntity,
    }

    impl LocalEntityAndGlobalEntityConverter for PointsAt {
        fn global_entity_to_host_entity(
            &self,
            _: &GlobalEntity,
        ) -> Result<crate::HostEntity, EntityDoesNotExistError> {
            Err(EntityDoesNotExistError)
        }
        fn global_entity_to_remote_entity(
            &self,
            _: &GlobalEntity,
        ) -> Result<RemoteEntity, EntityDoesNotExistError> {
            Ok(self.referenced)
        }
        fn global_entity_to_owned_entity(
            &self,
            _: &GlobalEntity,
        ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
            Ok(self.referenced.to_host().copy_to_owned())
        }
        fn host_entity_to_global_entity(
            &self,
            _: &crate::HostEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            Err(EntityDoesNotExistError)
        }
        fn static_host_entity_to_global_entity(
            &self,
            _: &crate::HostEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            Err(EntityDoesNotExistError)
        }
        fn remote_entity_to_global_entity(
            &self,
            _: &RemoteEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            Ok(GlobalEntity::from_u64(99))
        }
        fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
            *entity
        }
    }

    impl crate::LocalEntityAndGlobalEntityConverterMut for PointsAt {
        fn get_or_reserve_entity(
            &mut self,
            _: &GlobalEntity,
        ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
            Ok(self.referenced.to_host().copy_to_owned())
        }
    }

    /// A `Haunt` pointing at *something*. Which entity it names on the wire is
    /// decided by the converter it is written through, not here.
    fn a_haunt(value: u8) -> Haunt {
        let mut component = Haunt::new_complete(value);
        component
            .target
            .set(&crate::world::test_world::IdentityConverter, &99);
        component
    }

    /// Serializes a `Haunt` naming `referenced` and reads it back through
    /// `converter`, so a component that names an entity the converter cannot
    /// resolve comes back still waiting on it.
    fn a_haunt_component_pointing_at(
        component_kinds: &ComponentKinds,
        value: u8,
        referenced: RemoteEntity,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Box<dyn Replicate> {
        use naia_serde::{BitReader, BitWriter};

        let mut writer = BitWriter::new();
        a_haunt(value).write(component_kinds, &mut writer, &mut PointsAt { referenced });
        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        component_kinds
            .read(&mut reader, converter)
            .expect("a freshly written component should read back")
    }

    /// The same, as a wire update rather than a whole component.
    fn a_haunt_update_pointing_at(
        component_kinds: &ComponentKinds,
        value: u8,
        referenced: RemoteEntity,
    ) -> PendingComponentUpdate {
        use naia_serde::{BitReader, BitWriter};

        let component = a_haunt(value);
        let mut writer = BitWriter::new();
        ComponentKind::of::<Haunt>().ser(component_kinds, &mut writer);
        let mut diff_mask = crate::DiffMask::new(component.diff_mask_size());
        for index in 0..(diff_mask.byte_number() * 8) {
            diff_mask.set_bit(index, true);
        }
        component.write_update(&diff_mask, &mut writer, &mut PointsAt { referenced });

        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        component_kinds
            .read_create_update(&mut reader)
            .expect("a freshly written update should read back")
    }

    /// An insert whose component names an entity that has not arrived yet is
    /// held rather than applied, and lands when that entity spawns.
    #[test]
    fn a_component_naming_an_unknown_entity_waits_for_it() {
        let mut fixture = Fixture::new(HostType::Client);
        fixture.kinds.add_component::<Haunt>();
        let global = fixture.spawn_with_ghost(remote(1));

        let waiting = a_haunt_component_pointing_at(
            &fixture.kinds,
            5,
            remote(2),
            fixture.map.entity_converter(),
        );
        let mut components: HashMap<(OwnedLocalEntity, ComponentKind), Box<dyn Replicate>> =
            HashMap::new();
        components.insert(
            (remote(1).copy_to_owned(), ComponentKind::of::<Haunt>()),
            waiting,
        );

        let events = fixture.deliver_with(
            vec![EntityMessage::InsertComponent(
                remote(1),
                ComponentKind::of::<Haunt>(),
            )],
            &mut components,
        );
        assert!(
            summarize(&events).is_empty(),
            "the component names an entity this peer has not seen",
        );

        let events = fixture.deliver(vec![EntityMessage::Spawn(remote(2))]);
        let spawned = *fixture.map.global_entity_from_remote(&remote(2)).unwrap();
        assert_eq!(
            summarize(&events),
            vec![(Some(EntityMessageType::Spawn), spawned)]
        );

        // The connection tells the manager about the spawn separately: the
        // Spawn arm maps the entity, `spawn_entity` is what releases whatever
        // was waiting on it.
        fixture.manager.spawn_entity(&remote(2));
        let events = fixture.deliver(Vec::new());
        assert_eq!(
            summarize(&events),
            vec![(Some(EntityMessageType::InsertComponent), global)],
            "the entity arriving must release the insert that was waiting on it",
        );
    }

    /// An update whose field names an entity that has not arrived yet is held
    /// the same way, and raises its UpdateComponent event on release.
    #[test]
    fn an_update_naming_an_unknown_entity_waits_for_it() {
        let mut fixture = Fixture::new(HostType::Client);
        fixture.kinds.add_component::<Haunt>();
        let global = fixture.spawn_with_ghost(remote(1));

        let present = a_haunt_component_pointing_at(
            &fixture.kinds,
            1,
            remote(1),
            fixture.map.entity_converter(),
        );
        let mut components: HashMap<(OwnedLocalEntity, ComponentKind), Box<dyn Replicate>> =
            HashMap::new();
        components.insert(
            (remote(1).copy_to_owned(), ComponentKind::of::<Haunt>()),
            present,
        );
        fixture.deliver_with(
            vec![EntityMessage::InsertComponent(
                remote(1),
                ComponentKind::of::<Haunt>(),
            )],
            &mut components,
        );

        let update = a_haunt_update_pointing_at(&fixture.kinds, 5, remote(2));
        let events = fixture.deliver_updates(vec![(0, remote(1).copy_to_owned(), update)]);
        assert_eq!(
            summarize(&events),
            vec![(None, global)],
            "the half of the update that names nothing applies straight away",
        );

        fixture.deliver(vec![EntityMessage::Spawn(remote(2))]);
        fixture.manager.spawn_entity(&remote(2));
        let events = fixture.deliver(Vec::new());
        assert_eq!(
            summarize(&events),
            vec![(None, global)],
            "the entity arriving must release the field that was waiting on it",
        );
    }

    /// An update that outruns its own entity's spawn is held until the spawn
    /// catches up — a separate queue from the one above.
    #[test]
    fn an_update_that_outruns_its_own_entitys_spawn_is_held() {
        let mut fixture = Fixture::new(HostType::Client);

        let update = crate::world::test_world::full_update(&kinds(), &Ghost::new_complete(9));
        let events = fixture.deliver_updates(vec![(0, remote(1).copy_to_owned(), update)]);
        assert!(
            summarize(&events).is_empty(),
            "there is no entity to apply the update to yet",
        );

        let global = fixture.spawn_with_ghost(remote(1));
        fixture.manager.spawn_entity(&remote(1));
        let events = fixture.deliver_updates(Vec::new());
        assert_eq!(
            summarize(&events),
            vec![(None, global)],
            "the spawn must release the update that was waiting for it",
        );
    }

    // -- the entity channels the connection reaches for ---------------------

    #[test]
    fn a_channel_exists_only_for_an_entity_the_peer_has_sent() {
        let mut fixture = Fixture::new(HostType::Client);
        fixture.spawn_with_ghost(remote(1));

        assert!(fixture.manager.has_entity_channel(&remote(1)));
        assert!(!fixture.manager.has_entity_channel(&remote(2)));
        assert!(fixture.manager.get_entity_channel_mut(&remote(1)).is_some());
        assert!(fixture.manager.get_entity_channel_mut(&remote(2)).is_none());
        assert_eq!(
            fixture.manager.extract_component_kinds(&remote(1)),
            HashSet::from([ghost()]),
        );

        let channel = fixture.manager.remove_entity_channel(&remote(1));
        assert!(
            !fixture.manager.has_entity_channel(&remote(1)),
            "a channel lifted out for migration must not still be findable",
        );
        fixture.manager.insert_entity_channel(remote(1), channel);
        assert!(fixture.manager.has_entity_channel(&remote(1)));
    }
}
