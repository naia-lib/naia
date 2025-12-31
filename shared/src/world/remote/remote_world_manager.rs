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
    ComponentKind, ComponentKinds, ComponentUpdate, EntityAndGlobalEntityConverter,
    EntityAuthStatus, EntityCommand, EntityMessage, EntityMessageReceiver, GlobalEntity,
    GlobalEntitySpawner, GlobalWorldManagerType, HostType, LocalEntityAndGlobalEntityConverter,
    LocalEntityMap, MessageIndex, OwnedLocalEntity, Replicate, Tick, WorldMutType,
};

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

    pub fn entity_waitlist(&self) -> &RemoteEntityWaitlist {
        self.waitlist.entity_waitlist()
    }

    pub fn entity_waitlist_mut(&mut self) -> &mut RemoteEntityWaitlist {
        self.waitlist.entity_waitlist_mut()
    }

    pub(crate) fn register_authed_entity(&mut self, remote_entity: &RemoteEntity) {
        let Some(authed_entities) = self.authed_entities_opt.as_mut() else {
            return;
        };

        authed_entities.insert(*remote_entity);
    }

    pub(crate) fn deregister_authed_entity(&mut self, remote_entity: &RemoteEntity) {
        let Some(authed_entities) = self.authed_entities_opt.as_mut() else {
            return;
        };

        authed_entities.remove(remote_entity);
    }

    pub(crate) fn append_updatable_world(
        &self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        updatable_world: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) {
        let Some(authed_entities) = self.authed_entities_opt.as_ref() else {
            return;
        };

        for remote_entity in authed_entities {
            let Some(remote_channel) = self.remote_engine.get_world().get(remote_entity) else {
                continue;
            };
            let global_entity = local_converter
                .remote_entity_to_global_entity(remote_entity)
                .unwrap();
            let remote_component_kinds = remote_channel.component_kinds();
            if let Some(existing) = updatable_world.get_mut(&global_entity) {
                existing.extend(&remote_component_kinds);
            } else {
                updatable_world.insert(global_entity, remote_component_kinds);
            }
        }
    }

    pub fn take_outgoing_commands(&mut self) -> Vec<EntityCommand> {
        self.remote_engine.take_outgoing_commands()
    }

    pub fn send_entity_command(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        command: EntityCommand,
    ) {
        let global_entity = command.entity();
        let remote_entity = converter
            .global_entity_to_remote_entity(&global_entity)
            .unwrap();
        self.remote_engine
            .send_entity_command(remote_entity, command);
    }

    pub(crate) fn send_auth_command(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        command: EntityCommand,
    ) {
        let global_entity = command.entity();
        let remote_entity = converter
            .global_entity_to_remote_entity(&global_entity)
            .unwrap(); // error triggered here
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

    pub fn spawn_entity(
        &mut self,
        // converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &RemoteEntity,
    ) {
        self.waitlist.spawn_entity(&self.remote_engine, entity);
    }

    pub fn despawn_entity(
        &mut self,
        _local_entity_map: &mut LocalEntityMap,
        entity: &RemoteEntity,
    ) {
        self.waitlist.despawn_entity(entity);
    }

    pub fn take_incoming_events<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        spawner: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        local_entity_map: &mut LocalEntityMap,
        component_kinds: &ComponentKinds,
        world: &mut W,
        now: &Instant,
        incoming_components: &mut HashMap<(OwnedLocalEntity, ComponentKind), Box<dyn Replicate>>,
        incoming_updates: Vec<(Tick, OwnedLocalEntity, ComponentUpdate)>,
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
                    if local_entity_map.contains_remote_entity(&remote_entity) {
                        // mapped remote entity already when reserving global entity
                    } else {
                        local_entity_map.insert_with_remote_entity(global_entity, remote_entity);
                    }

                    self.incoming_events.push(EntityEvent::Spawn(global_entity));
                }
                EntityMessage::Despawn(remote_entity) => {
                    let global_entity = local_entity_map.remove_by_remote_entity(&remote_entity);
                    let world_entity = spawner.global_entity_to_entity(&global_entity).unwrap();

                    // Generate event for each component, handing references off just in
                    // case
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
            // let name = component.name();
            // warn!(
            //     "Remote World Manager: waitlisting entity {:?}'s component {:?} for insertion. Waiting on Entities: {:?}",
            //     global_entity, &name, remote_entity_set,
            // );

            self.waitlist.waitlist_queue_entity(
                &self.remote_engine,
                &entity,
                component,
                component_kind,
                &remote_entity_set,
            );
        } else {
            self.finish_insert(
                world,
                converter,
                entity,
                &world_entity,
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

        world.insert_boxed_component(&world_entity, component);

        let global_entity = converter.remote_entity_to_global_entity(&entity).unwrap();

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
        if self.waitlist.process_remove(&entity, &component_kind) {
            return;
        }
        // Remove from world
        if let Some(component) = world.remove_component_of_kind(&world_entity, &component_kind) {
            // Send out event
            if let Ok(global_entity) = converter.remote_entity_to_global_entity(&entity) {
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
            let global_entity = local_converter
                .remote_entity_to_global_entity(&entity)
                .unwrap();
            let world_entity = world_converter
                .global_entity_to_entity(&global_entity)
                .unwrap();
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
        incoming_updates: Vec<(Tick, OwnedLocalEntity, ComponentUpdate)>,
    ) {
        self.process_ready_updates(
            local_converter,
            world_converter,
            component_kinds,
            world,
            incoming_updates,
        );
        self.process_waitlist_updates(local_converter, world_converter, world, now);
    }

    /// Process component updates from raw bits for a given entity
    fn process_ready_updates<WE: Copy + Eq + Hash + Send + Sync, W: WorldMutType<WE>>(
        &mut self,
        local_converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_converter: &dyn EntityAndGlobalEntityConverter<WE>,
        component_kinds: &ComponentKinds,
        world: &mut W,
        incoming_updates: Vec<(Tick, OwnedLocalEntity, ComponentUpdate)>,
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

    /// Get auth status of a remote entity's channel (for testing)
    pub fn get_entity_auth_status(&self, entity: &RemoteEntity) -> Option<EntityAuthStatus> {
        self.remote_engine.get_entity_auth_status(entity)
    }
}

impl InScopeEntities<RemoteEntity> for RemoteWorldManager {
    fn has_entity(&self, entity: &RemoteEntity) -> bool {
        self.remote_engine.has_entity(entity)
    }
}
