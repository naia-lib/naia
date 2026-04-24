use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    sync::RwLockReadGuard,
    time::Duration,
};

use log::info;
use naia_socket_shared::Instant;

use crate::world::sync::RemoteEntityChannel;
use crate::world::update::entity_update_manager::EntityUpdateManager;
use crate::{
    messages::channels::receivers::reliable_receiver::ReliableReceiver,
    sequence_list::SequenceList,
    types::{HostType, PacketIndex},
    world::{
        entity::entity_converters::GlobalWorldManagerType,
        host::host_world_manager::{CommandId, HostWorldManager},
        remote::remote_entity_waitlist::{RemoteEntityWaitlist, WaitlistStore},
        sync::HostEntityChannel,
    },
    ChannelSender, ComponentKind, ComponentKinds, ComponentUpdate, DiffMask,
    EntityAndGlobalEntityConverter, EntityAuthStatus, EntityCommand, EntityConverterMut,
    EntityEvent, EntityMessage, EntityMessageType, GlobalEntity, GlobalEntitySpawner, HostEntity,
    InScopeEntities, LocalEntityAndGlobalEntityConverter, LocalEntityMap, MessageIndex,
    OwnedLocalEntity, PacketNotifiable, ReliableSender, RemoteEntity, RemoteWorldManager,
    Replicate, Tick, WorldMutType, WorldRefType,
};

cfg_if! {
    if #[cfg(feature = "e2e_debug")] {
        use crate::world::{
            host::host_world_manager::SubCommandId,
            sync::remote_entity_channel::EntityChannelState,
        };
    }
}

const RESEND_COMMAND_RTT_FACTOR: f32 = 1.5;
const COMMAND_RECORD_TTL: Duration = Duration::from_secs(60);

pub struct LocalWorldManager {
    entity_map: LocalEntityMap,
    sender: ReliableSender<EntityCommand>,
    sent_command_packets:
        SequenceList<(Instant, Vec<(CommandId, EntityMessage<OwnedLocalEntity>)>)>,
    receiver: ReliableReceiver<EntityMessage<OwnedLocalEntity>>,

    host: HostWorldManager,
    remote: RemoteWorldManager,
    updater: EntityUpdateManager,

    /// Entities with ScopeExit::Persist that are currently out-of-scope.
    /// Replication is frozen for these entities until re-entry.
    paused_entities: HashSet<GlobalEntity>,

    // TODO: this is kind of specific to the receiver, put it somewhere else?
    incoming_components: HashMap<(OwnedLocalEntity, ComponentKind), Box<dyn Replicate>>,

    // TODO: this is kind of specific to the updater, put it somewhere else?
    incoming_updates: Vec<(Tick, OwnedLocalEntity, ComponentUpdate)>,
}

impl LocalWorldManager {
    pub fn new(
        address: &Option<SocketAddr>,
        host_type: HostType,
        user_key: u64,
        global_world_manager: &dyn GlobalWorldManagerType,
    ) -> Self {
        Self {
            entity_map: LocalEntityMap::new(host_type),
            sender: ReliableSender::new(RESEND_COMMAND_RTT_FACTOR),
            sent_command_packets: SequenceList::new(),
            receiver: ReliableReceiver::new(),

            host: HostWorldManager::new(host_type, user_key),
            remote: RemoteWorldManager::new(host_type),
            updater: EntityUpdateManager::new(address, global_world_manager),

            paused_entities: HashSet::new(),

            incoming_components: HashMap::new(),
            incoming_updates: Vec::new(),
        }
    }

    pub(crate) fn entity_waitlist_queue<T>(
        &mut self,
        remote_entity_set: &HashSet<RemoteEntity>,
        waitlist_store: &mut WaitlistStore<T>,
        message: T,
    ) {
        self.remote
            .entity_waitlist_queue(remote_entity_set, waitlist_store, message);
    }

    // EntityMap-focused

    pub fn entity_converter(&self) -> &dyn LocalEntityAndGlobalEntityConverter {
        self.entity_map.entity_converter()
    }

    pub fn entity_converter_mut<'a, 'b>(
        &'b mut self,
        global_world_manager: &'a dyn GlobalWorldManagerType,
    ) -> EntityConverterMut<'a, 'b> {
        self.host
            .entity_converter_mut(global_world_manager, &mut self.entity_map)
    }

    pub fn has_global_entity(&self, global_entity: &GlobalEntity) -> bool {
        let Ok(local_entity) = self.entity_map.global_entity_to_owned_entity(global_entity) else {
            return false;
        };
        return self.has_local_entity(&local_entity);
    }

    pub fn has_local_entity(&self, local_entity: &OwnedLocalEntity) -> bool {
        match local_entity {
            OwnedLocalEntity::Host(host_entity) => {
                self.host.has_entity(&HostEntity::new(*host_entity))
            }
            OwnedLocalEntity::Remote(remote_entity) => {
                self.remote.has_entity(&RemoteEntity::new(*remote_entity))
            }
        }
    }

    /// Get a reference to a HostEntityChannel (for testing)
    pub fn get_host_entity_channel(
        &self,
        entity: &HostEntity,
    ) -> Option<&crate::world::sync::HostEntityChannel> {
        self.host.get_entity_channel(entity)
    }

    /// Get a mutable reference to a HostEntityChannel (for testing)
    pub fn get_host_entity_channel_mut(
        &mut self,
        entity: &HostEntity,
    ) -> Option<&mut crate::world::sync::HostEntityChannel> {
        self.host.get_entity_channel_mut(entity)
    }

    // Host-focused

    pub fn has_host_entity(&self, host_entity: &HostEntity) -> bool {
        self.host.has_entity(&host_entity)
    }

    pub fn host_init_entity(
        &mut self,
        global_entity: &GlobalEntity,
        component_kinds: Vec<ComponentKind>,
    ) {
        if self
            .entity_map
            .global_entity_to_host_entity(global_entity)
            .is_err()
        {
            // this is done because `host_reserve_entity()` may have been called previously!
            let host_entity = self.host.host_generate_entity();
            self.entity_map
                .insert_with_host_entity(*global_entity, host_entity);
        }
        self.host.init_entity_send_host_commands(
            &self.entity_map,
            global_entity,
            component_kinds,
            &mut self.updater,
        );
    }

    /// BULLETPROOF: Migrate entity from remote (client) control to host (server) control
    ///
    /// This method performs a complete, atomic migration of an entity from client control
    /// to server control, including:
    /// - Force-draining all buffered operations
    /// - Preserving component state
    /// - Installing entity redirects
    /// - Updating command references
    /// - Cleaning up old entity channels
    ///
    /// # Errors
    ///
    /// This method will panic if:
    /// - The entity doesn't exist in the local entity map
    /// - The entity is not currently remote-owned
    /// - Any step of the migration process fails
    ///
    /// # Safety
    ///
    /// This method is designed to be atomic - either the entire migration succeeds
    /// or the system remains in a consistent state. No partial migrations are possible.
    pub fn migrate_entity_remote_to_host(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, String> {
        // Validate entity exists and is remote-owned
        let Some(local_entity_record) = self.entity_map.remove_by_global_entity(global_entity)
        else {
            return Err(format!(
                "Entity does not exist in local entity map: {:?}",
                global_entity
            ));
        };

        if !local_entity_record.is_remote_owned() {
            // Restore the entity record since we removed it
            self.entity_map
                .insert_with_remote_entity(*global_entity, local_entity_record.remote_entity());
            return Err(format!("Entity is not remote-owned: {:?}", global_entity));
        }
        let old_remote_entity = local_entity_record.remote_entity();

        // create new host entity, insert into local entity map
        let new_host_entity = self.host.host_generate_entity();

        self.entity_map
            .insert_with_host_entity(*global_entity, new_host_entity);

        // CRITICAL: After migration, global_entity_to_remote_entity() must fail for this global_entity
        // remove_by_global_entity should have removed the remote mapping, but verify it's gone
        // This prevents SetAuthority from encoding via stale global->remote mapping
        // Double-check: ensure old remote mapping is completely removed from remote_to_global
        self.entity_map
            .remove_remote_mapping_if_exists(&old_remote_entity);

        // Verify the invariant: after migration, global_entity should NOT convert to remote_entity
        // This is a defensive check - if this fails, there's a bug in remove_by_global_entity
        debug_assert!(
            self.entity_map
                .entity_converter()
                .global_entity_to_remote_entity(global_entity)
                .is_err(),
            "After migration, global_entity_to_remote_entity must fail for migrated entity"
        );

        // BULLETPROOF: Step 1: Force-drain all buffers in RemoteEntityChannel
        // This ensures all pending operations are processed before migration
        self.remote.force_drain_entity_buffers(&old_remote_entity);

        // BULLETPROOF: Step 2: Extract component state from RemoteEntityChannel
        // This preserves the current component state during migration
        let component_kinds = self.remote.extract_component_kinds(&old_remote_entity);

        // BULLETPROOF: Step 3: Remove RemoteEntityChannel from RemoteEngine
        // This must succeed or we're in an inconsistent state
        let _old_remote_channel = self.remote.remove_entity_channel(&old_remote_entity);

        // BULLETPROOF: Step 4: Create new HostEntityChannel with extracted component state
        // This creates the new server-side entity channel with preserved state
        let new_host_channel =
            HostEntityChannel::new_with_components(self.entity_map.host_type(), component_kinds);

        // BULLETPROOF: Step 5: Insert new HostEntityChannel into HostEngine
        // This must succeed or we lose the entity channel
        self.host
            .insert_entity_channel(new_host_entity, new_host_channel);

        // BULLETPROOF: Step 6: Install entity redirect in LocalEntityMap
        // This allows old entity references to be automatically updated
        let old_entity = OwnedLocalEntity::Remote(old_remote_entity.value());
        let new_entity = OwnedLocalEntity::Host(new_host_entity.value());
        self.entity_map
            .install_entity_redirect(old_entity, new_entity);

        // BULLETPROOF: Step 7: Update all references in sent_command_packets
        // This ensures pending commands are sent to the correct entity
        self.update_sent_command_entity_refs(global_entity, old_entity, new_entity);

        // BULLETPROOF: Step 8: Clean up old remote entity
        // This removes the old client-side entity channel
        self.remote
            .despawn_entity(&mut self.entity_map, &old_remote_entity);

        Ok(new_host_entity)
    }

    // only server sends this
    pub fn host_send_enable_delegation(&mut self, global_entity: &GlobalEntity) {
        let command = EntityCommand::EnableDelegation(None, *global_entity);
        self.host.send_command(&self.entity_map, command);
    }

    // Force the HostEntityChannel into Delegated state without sending a message
    // Used by server after migration to prepare channel for MigrateResponse
    pub fn host_local_enable_delegation(&mut self, host_entity: &HostEntity) {
        let Some(channel) = self.host.get_entity_channel_mut(host_entity) else {
            panic!(
                "Cannot enable delegation on non-existent HostEntity: {:?}",
                host_entity
            );
        };
        channel.local_enable_delegation();
    }

    // only server sends this
    pub fn host_send_migrate_response(
        &mut self,
        global_entity: &GlobalEntity,
        old_remote_entity: &RemoteEntity, // Server's RemoteEntity (represents client's entity)
        new_host_entity: &HostEntity,     // Server's new HostEntity (what server created)
    ) {
        // EntityCommand::MigrateResponse signature: (subid, global, RemoteEntity, HostEntity)
        // These types are from SERVER perspective and will be reinterpreted by CLIENT
        let command = EntityCommand::MigrateResponse(
            None,
            *global_entity,
            *old_remote_entity,
            *new_host_entity,
        );
        self.host.send_command(&self.entity_map, command);
    }

    #[track_caller]
    pub fn host_send_set_auth(
        &mut self,
        global_entity: &GlobalEntity,
        auth_status: EntityAuthStatus,
    ) {
        #[cfg(feature = "e2e_debug")]
        {
            crate::e2e_trace!(
                "[SERVER_SEND] SetAuthority entity={:?} status={:?}",
                global_entity,
                auth_status
            );
        }
        let Ok(local_entity) = self.entity_map.global_entity_to_owned_entity(global_entity) else {
            panic!("Attempting to send SetAuthority for entity which does not exist in local entity map! {:?}", global_entity);
        };

        let command = EntityCommand::SetAuthority(None, *global_entity, auth_status);
        if local_entity.is_host() {
            self.host.send_command(&self.entity_map, command);
        } else {
            // For RemoteEntity, use remote.send_auth_command (similar to send_publish)
            self.remote
                .send_auth_command(self.entity_map.entity_converter(), command);
        }
    }

    pub fn host_reserve_entity(&mut self, global_entity: &GlobalEntity) -> HostEntity {
        self.host
            .host_reserve_entity(&mut self.entity_map, global_entity)
    }

    pub fn host_remove_reserved_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Option<HostEntity> {
        self.host.host_removed_reserved_entity(global_entity)
    }

    pub(crate) fn insert_sent_command_packet(&mut self, packet_index: &PacketIndex, now: Instant) {
        if !self
            .sent_command_packets
            .contains_scan_from_back(packet_index)
        {
            self.sent_command_packets
                .insert_scan_from_back(*packet_index, (now, Vec::new()));
        }
    }

    pub(crate) fn record_command_written(
        &mut self,
        packet_index: &PacketIndex,
        command_id: &CommandId,
        message: EntityMessage<OwnedLocalEntity>,
    ) {
        let (_, sent_actions_list) = self
            .sent_command_packets
            .get_mut_scan_from_back(packet_index)
            .unwrap();
        sent_actions_list.push((*command_id, message));
    }

    // Remote-focused

    #[allow(dead_code)]
    pub(crate) fn has_remote_entity(&self, remote_entity: &RemoteEntity) -> bool {
        self.remote.has_entity(remote_entity)
    }

    pub fn remote_entities(&self) -> Vec<GlobalEntity> {
        self.entity_map.remote_entities()
    }

    #[cfg(feature = "e2e_debug")]
    pub fn debug_remote_channel_diagnostic(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Option<(
        EntityChannelState,
        (SubCommandId, usize, Option<SubCommandId>, usize),
    )> {
        self.remote.debug_channel_diagnostic(remote_entity)
    }

    #[cfg(feature = "e2e_debug")]
    pub fn debug_remote_channel_snapshot(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Option<(
        EntityChannelState,
        Option<MessageIndex>,
        usize,
        Option<(MessageIndex, EntityMessageType)>,
        Option<MessageIndex>,
    )> {
        self.remote.debug_channel_snapshot(remote_entity)
    }

    // only client sends this, after receiving enabledelegation message from server
    pub fn send_enable_delegation_response(&mut self, global_entity: &GlobalEntity) {
        let command = EntityCommand::EnableDelegationResponse(None, *global_entity);
        self.remote.send_auth_command(&self.entity_map, command);
    }

    pub fn remote_send_request_auth(&mut self, global_entity: &GlobalEntity) {
        let command = EntityCommand::RequestAuthority(None, *global_entity);
        self.remote.send_auth_command(&self.entity_map, command);
    }

    /// Update the RemoteEntityChannel's AuthChannel status (used after migration)
    pub fn remote_receive_set_auth(
        &mut self,
        global_entity: &GlobalEntity,
        auth_status: EntityAuthStatus,
    ) {
        let remote_entity = self
            .entity_map
            .entity_converter()
            .global_entity_to_remote_entity(global_entity)
            .unwrap();
        self.remote
            .receive_set_auth_status(remote_entity, auth_status);
    }

    /// Get auth status of a remote entity's channel (for testing)
    pub fn get_remote_entity_auth_status(
        &self,
        global_entity: &GlobalEntity,
    ) -> Option<EntityAuthStatus> {
        let Ok(OwnedLocalEntity::Remote(remote_entity_value)) =
            self.entity_map.global_entity_to_owned_entity(global_entity)
        else {
            return None;
        };
        self.remote
            .get_entity_auth_status(&RemoteEntity::new(remote_entity_value))
    }

    pub fn entity_waitlist_mut(&mut self) -> &mut RemoteEntityWaitlist {
        self.remote.entity_waitlist_mut()
    }

    /// Buffer an incoming message for processing (exposed for testing)
    pub fn receiver_buffer_message(
        &mut self,
        id: MessageIndex,
        msg: EntityMessage<OwnedLocalEntity>,
    ) {
        // if msg.get_type() != EntityMessageType::Noop {
        //     use log::info;
        //     info!(
        //         "LocalWorldManager::receiver_buffer_message(id={}, msg_type={:?})",
        //         id,
        //         msg.get_type()
        //     );
        // }

        self.receiver.buffer_message(id, msg);
    }

    pub(crate) fn insert_received_component(
        &mut self,
        local_entity: &OwnedLocalEntity,
        component_kind: &ComponentKind,
        component: Box<dyn Replicate>,
    ) {
        self.incoming_components
            .insert((*local_entity, *component_kind), component);
    }

    pub(crate) fn insert_received_update(
        &mut self,
        tick: Tick,
        local_entity: &OwnedLocalEntity,
        component_update: ComponentUpdate,
    ) {
        self.incoming_updates
            .push((tick, *local_entity, component_update));
    }

    pub fn take_incoming_events<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        spawner: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        component_kinds: &ComponentKinds,
        world: &mut W,
        now: &Instant,
    ) -> Vec<EntityEvent> {
        let incoming_messages = self.receiver.receive_messages();
        let mut incoming_host_messages = Vec::new();
        let mut incoming_remote_messages = Vec::new();

        for (id, incoming_message) in incoming_messages {
            if incoming_message.get_type() == EntityMessageType::Noop {
                continue; // skip noop messages
            }

            // use log::info;
            // info!(
            //     "LocalWorldManager::take_incoming_events - processing message: id={}, type={:?}",
            //     id,
            //     incoming_message.get_type()
            // );

            let Some(local_entity) = incoming_message.entity() else {
                panic!(
                    "Received message without an entity! Message: {:?}",
                    incoming_message
                );
            };
            match local_entity {
                OwnedLocalEntity::Host(host_entity) => {
                    // Host entity message
                    let host_entity = HostEntity::new(host_entity);
                    incoming_host_messages.push((id, incoming_message.with_entity(host_entity)));
                }
                OwnedLocalEntity::Remote(remote_entity) => {
                    // Remote entity message
                    let remote_entity = RemoteEntity::new(remote_entity);
                    // Count when Spawn is routed to incoming_remote_messages
                    #[cfg(feature = "e2e_debug")]
                    if incoming_message.get_type() == EntityMessageType::Spawn {
                        extern "Rust" {
                            fn client_routed_remote_spawn_increment();
                        }
                        unsafe {
                            client_routed_remote_spawn_increment();
                        }
                    }
                    incoming_remote_messages
                        .push((id, incoming_message.with_entity(remote_entity)));
                }
            }
        }

        let host_events = self.host.take_incoming_events(
            spawner,
            global_world_manager,
            &self.entity_map,
            world,
            incoming_host_messages,
        );
        let mut remote_events = self.remote.take_incoming_events(
            spawner,
            global_world_manager,
            &mut self.entity_map,
            component_kinds,
            world,
            now,
            &mut self.incoming_components,
            std::mem::take(&mut self.incoming_updates),
            incoming_remote_messages,
        );

        let mut incoming_events = host_events;
        incoming_events.append(&mut remote_events);

        incoming_events
    }

    pub fn register_authed_entity(
        &mut self,
        global_manager: &dyn GlobalWorldManagerType,
        global_entity: &GlobalEntity,
    ) {
        // info!("Registering authed entity: {:?}", global_entity);

        if let Ok(remote_entity) = self
            .entity_map
            .global_entity_to_remote_entity(global_entity)
        {
            self.remote.register_authed_entity(&remote_entity);
        }

        let Some(component_kinds) = global_manager.component_kinds(global_entity) else {
            // entity has no components yet
            return;
        };

        for component_kind in component_kinds.iter() {
            self.updater
                .register_component(global_entity, component_kind);
        }
    }

    pub fn deregister_authed_entity(
        &mut self,
        global_manager: &dyn GlobalWorldManagerType,
        global_entity: &GlobalEntity,
    ) {
        // info!("Deregistering delegated entity updates for {:?}", global_entity);

        if let Ok(remote_entity) = self
            .entity_map
            .global_entity_to_remote_entity(global_entity)
        {
            self.remote.deregister_authed_entity(&remote_entity);
        }

        let Some(component_kinds) = global_manager.component_kinds(global_entity) else {
            // entity has no components yet
            return;
        };

        for component_kind in component_kinds.iter() {
            self.updater
                .deregister_component(global_entity, component_kind);
        }
    }

    pub fn remote_spawn_entity(&mut self, global_entity: &GlobalEntity) {
        let remote_entity = self
            .entity_map
            .global_entity_to_remote_entity(global_entity)
            .unwrap();
        self.remote.spawn_entity(&remote_entity);
    }

    pub fn remote_despawn_entity(&mut self, global_entity: &GlobalEntity) {
        let remote_entity = self
            .entity_map
            .global_entity_to_remote_entity(global_entity)
            .unwrap();
        self.remote
            .despawn_entity(&mut self.entity_map, &remote_entity);
    }

    // Update-focused

    pub(crate) fn get_diff_mask(
        &self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> RwLockReadGuard<'_, DiffMask> {
        self.updater.get_diff_mask(global_entity, component_kind)
    }

    pub(crate) fn record_update(
        &mut self,
        now: &Instant,
        packet_index: &PacketIndex,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        diff_mask: DiffMask,
    ) {
        self.updater
            .record_update(now, packet_index, global_entity, component_kind, diff_mask);
    }

    // Joint router

    pub fn despawn_entity(&mut self, global_entity: &GlobalEntity) {
        // Clean up pause state if entity was Paused (ScopeExit::Persist)
        self.paused_entities.remove(global_entity);

        let Ok(local_entity) = self.entity_map.global_entity_to_owned_entity(global_entity) else {
            panic!(
                "Attempting to despawn entity which does not exist in local entity map! {:?}",
                global_entity
            );
        };
        if local_entity.is_host() {
            self.host
                .send_command(&self.entity_map, EntityCommand::Despawn(*global_entity));
        } else {
            self.remote
                .send_entity_command(&self.entity_map, EntityCommand::Despawn(*global_entity));
        }
    }

    /// Pause replication for a `ScopeExit::Persist` entity that has left scope.
    /// The entity stays in the client's entity pool; no further updates are sent
    /// until `resume_entity` is called on re-entry.
    pub fn pause_entity(&mut self, global_entity: &GlobalEntity) {
        self.paused_entities.insert(*global_entity);
    }

    /// Resume replication for a paused `ScopeExit::Persist` entity that has re-entered scope.
    /// Accumulated deltas will be delivered on the next update cycle.
    pub fn resume_entity(&mut self, global_entity: &GlobalEntity) {
        self.paused_entities.remove(global_entity);
    }

    pub fn is_entity_paused(&self, global_entity: &GlobalEntity) -> bool {
        self.paused_entities.contains(global_entity)
    }

    pub fn insert_component(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        let Ok(local_entity) = self.entity_map.global_entity_to_owned_entity(global_entity) else {
            panic!("Attempting to insert component for entity which does not exist in local entity map! {:?}", global_entity);
        };
        if local_entity.is_host() {
            // Register component immediately when it comes into scope (not waiting for delivery confirmation)
            // This ensures mutations can set the diff mask right away
            self.updater
                .register_component(global_entity, component_kind);
            self.host.send_command(
                &self.entity_map,
                EntityCommand::InsertComponent(*global_entity, *component_kind),
            );
        } else {
            self.remote.send_entity_command(
                &self.entity_map,
                EntityCommand::InsertComponent(*global_entity, *component_kind),
            );
        }
    }

    pub fn remove_component(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        let Ok(local_entity) = self.entity_map.global_entity_to_owned_entity(global_entity) else {
            panic!("Attempting to remove component for entity which does not exist in local entity map! {:?}", global_entity);
        };
        if local_entity.is_host() {
            self.host.send_command(
                &self.entity_map,
                EntityCommand::RemoveComponent(*global_entity, *component_kind),
            );
        } else {
            self.remote.send_entity_command(
                &self.entity_map,
                EntityCommand::RemoveComponent(*global_entity, *component_kind),
            );
        }
    }

    pub fn send_publish(&mut self, host_type: HostType, global_entity: &GlobalEntity) {
        let Ok(local_entity) = self.entity_map.global_entity_to_owned_entity(global_entity) else {
            panic!(
                "Attempting to publish entity which does not exist in local entity map! {:?}",
                global_entity
            );
        };
        let host_owned = match (host_type, local_entity.is_host()) {
            (HostType::Server, true) => {
                panic!("Server-owned Entities are published by default, invalid!")
            }
            (HostType::Client, false) => {
                panic!("Server-owned Entities are published by default, invalid!")
            }
            (HostType::Server, false) => false, // todo!("server is attempting to publish a client-owned non-public remote entity"),
            (HostType::Client, true) => true, // todo!("client is attempting to publish a client-owned host entity"),
        };

        let command = EntityCommand::Publish(None, *global_entity);
        if host_owned {
            self.host.send_command(&self.entity_map, command);
        } else {
            self.remote
                .send_auth_command(self.entity_map.entity_converter(), command);
        }
    }

    pub fn send_unpublish(&mut self, host_type: HostType, global_entity: &GlobalEntity) {
        let Ok(local_entity) = self.entity_map.global_entity_to_owned_entity(global_entity) else {
            panic!(
                "Attempting to publish entity which does not exist in local entity map! {:?}",
                global_entity
            );
        };
        let host_owned = match (host_type, local_entity.is_host()) {
            (HostType::Server, true) => panic!("Server-owned Entities cannot be unpublished!"),
            (HostType::Client, false) => panic!("Server-owned Entities cannot be unpublished!"),
            (HostType::Server, false) => false, // todo!("server is attempting to unpublish a client-owned public entity"),
            (HostType::Client, true) => true, // todo!("client is attempting to unpublish a client-owned public entity"),
        };
        let command = EntityCommand::Unpublish(None, *global_entity);
        if host_owned {
            self.host.send_command(&self.entity_map, command);
        } else {
            self.remote
                .send_auth_command(self.entity_map.entity_converter(), command);
        }
    }

    pub fn send_enable_delegation(
        &mut self,
        host_type: HostType,
        origin_is_owning_client: bool,
        global_entity: &GlobalEntity,
    ) {
        // let is_delegated = self.entity_map.global_entity_is_delegated(global_entity);
        // if is_delegated {
        //     panic!("Entity {:?} is already delegated!", global_entity);
        // }
        let Ok(local_entity) = self.entity_map.global_entity_to_owned_entity(global_entity) else {
            panic!("Attempting to enable delegation for entity which does not exist in local entity map! {:?}", global_entity);
        };
        let host_owned = match (host_type, local_entity.is_host(), origin_is_owning_client) {
            (HostType::Server, false, true) => {
                panic!("Client cannot originate enable delegation for ANOTHER client-owned entity!")
            }
            (HostType::Client, _, false) => {
                panic!("Client must be the owning client to enable delegation!")
            }
            (HostType::Client, false, true) => {
                panic!("Client cannot enable delegation for a Server-owned entity")
            }

            (HostType::Server, true, true) => true, // todo!("server is proxying client-originating enable delegation message to client (entity should be host-owned here)"),
            (HostType::Server, true, false) => true, // todo!("server is enabling delegation for a server-owned entity (host owned)"),
            (HostType::Client, true, true) => true, // todo!("client is attempting to enable delegation for a client-owned entity (host owned)"),
            (HostType::Server, false, false) => false, // todo!("server is attempting to delegate a (hopefully published) client-owned entity (remote-owned entity"),
        };

        if host_owned {
            // Check if entity is already Published
            let host_entity = self
                .entity_map
                .global_entity_to_host_entity(global_entity)
                .expect("Host entity should exist");

            let is_published = if let Some(channel) = self.get_host_entity_channel(&host_entity) {
                use crate::world::sync::auth_channel::EntityAuthChannelState;
                let state = channel.auth_channel_state();
                state == EntityAuthChannelState::Published
                    || state == EntityAuthChannelState::Delegated
            } else {
                false
            };

            // Only send Publish if entity is NOT already Published
            if !is_published {
                let publish_command = EntityCommand::Publish(None, *global_entity);
                self.host.send_command(&self.entity_map, publish_command);
            }

            // Always send EnableDelegation (this will transition Published → Delegated)
            #[cfg(feature = "e2e_debug")]
            crate::e2e_trace!(
                "[SERVER_SEND] EnableDelegation entity={:?} callsite=send_enable_delegation(host)",
                global_entity
            );
            let enable_delegation_command = EntityCommand::EnableDelegation(None, *global_entity);
            self.host
                .send_command(&self.entity_map, enable_delegation_command);
        } else {
            #[cfg(feature = "e2e_debug")]
            crate::e2e_trace!("[SERVER_SEND] EnableDelegation entity={:?} callsite=send_enable_delegation(remote)", global_entity);
            let command = EntityCommand::EnableDelegation(None, *global_entity);
            self.remote
                .send_auth_command(self.entity_map.entity_converter(), command);
        }
    }

    #[track_caller]
    pub fn send_disable_delegation(&mut self, global_entity: &GlobalEntity) {
        #[cfg(feature = "e2e_debug")]
        {
            let caller = std::panic::Location::caller();
            crate::e2e_trace!(
                "[SERVER_SEND] DisableDelegation entity={:?} caller={}:{}",
                global_entity,
                caller.file(),
                caller.line()
            );
        }
        // only server should ever be able to call this, on host-owned (server-owned) entities
        let command = EntityCommand::DisableDelegation(None, *global_entity);
        self.host.send_command(&self.entity_map, command);
    }

    pub fn remote_send_release_auth(&mut self, global_entity: &GlobalEntity) {
        let command = EntityCommand::ReleaseAuthority(None, *global_entity);

        let host_owned = self
            .entity_map
            .global_entity_to_owned_entity(global_entity)
            .unwrap()
            .is_host();
        if host_owned {
            self.host.send_command(&self.entity_map, command);
        } else {
            self.remote
                .send_auth_command(self.entity_map.entity_converter(), command);
        }
    }

    // Joint

    pub fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        self.handle_dropped_command_packets(now);
        self.updater.handle_dropped_update_packets(now, rtt_millis);
    }

    fn handle_dropped_command_packets(&mut self, now: &Instant) {
        let mut pop = false;

        loop {
            if let Some((_, (time_sent, _))) = self.sent_command_packets.front() {
                if time_sent.elapsed(now) > COMMAND_RECORD_TTL {
                    pop = true;
                }
            } else {
                break;
            }
            if pop {
                self.sent_command_packets.pop_front();
            } else {
                break;
            }
        }

        // Also cleanup old entity redirects with the same TTL
        self.entity_map.cleanup_old_redirects(now, 60);
    }

    pub fn take_outgoing_events<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        now: &Instant,
        rtt_millis: &f32,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
    ) -> (
        VecDeque<(CommandId, EntityCommand)>,
        HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) {
        // get outgoing world commands
        let host_commands = self.host.take_outgoing_commands();
        let remote_commands = self.remote.take_outgoing_commands();
        for commands in [host_commands, remote_commands] {
            for command in commands {
                self.sender.send_message(command);
            }
        }
        self.sender.collect_messages(now, rtt_millis);
        let world_commands = self.sender.take_next_messages();

        // get update events
        let update_events = self.take_update_events(world, converter, global_world_manager);

        // return both
        (world_commands, update_events)
    }

    pub fn process_delivered_commands(&mut self) {
        self.host
            .process_delivered_commands(&mut self.entity_map, &mut self.updater);
    }

    pub fn take_update_events<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        world: &W,
        world_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
    ) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
        let mut updatable_world = self.host.get_updatable_world(&self.entity_map);
        let local_converter = self.entity_map.entity_converter();
        self.remote
            .append_updatable_world(local_converter, &mut updatable_world);
        updatable_world.retain(|ge, _| !self.paused_entities.contains(ge));
        self.updater.take_outgoing_events(
            world,
            world_converter,
            global_world_manager,
            updatable_world,
        )
    }

    // pub(crate) fn get_message_reader_helpers<'a, 'b, 'c, E: Copy + Eq + Hash + Sync + Send>(
    //     &'b mut self,
    //     spawner: &'b mut dyn GlobalEntitySpawner<E>
    // ) -> (GlobalEntityReserver<'a, 'b, 'c, E>, &'a mut EntityWaitlist<RemoteEntity>) {
    //     let remote= &mut self.remote;
    //     let entity_map = &mut self.entity_map;
    //     let reserver = remote.get_message_reader_helpers(entity_map, spawner);
    //     (reserver, remote.entity_waitlist_mut())
    // }

    pub fn get_message_processor_helpers(
        &mut self,
    ) -> (
        &dyn LocalEntityAndGlobalEntityConverter,
        &mut RemoteEntityWaitlist,
    ) {
        let entity_converter = self.entity_map.entity_converter();
        let entity_waitlist = self.remote.entity_waitlist_mut();
        (entity_converter, entity_waitlist)
    }

    fn host_notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some((_, command_list)) = self
            .sent_command_packets
            .remove_scan_from_front(&packet_index)
        {
            for (command_id, command) in command_list {
                if self.sender.deliver_message(&command_id).is_some() {
                    self.deliver_message(command_id, command);
                }
            }
        }
    }

    fn deliver_message(&mut self, id: CommandId, msg: EntityMessage<OwnedLocalEntity>) {
        if msg.is_noop() {
            return;
        }
        let Some(local_entity) = msg.entity() else {
            panic!("Delivered message without an entity! Message: {:?}", msg);
        };
        match local_entity {
            OwnedLocalEntity::Host(host_entity) => {
                // Host entity message
                let host_entity = HostEntity::new(host_entity);
                self.host.deliver_message(id, msg.with_entity(host_entity));
            }
            OwnedLocalEntity::Remote(remote_entity) => {
                // Remote entity message
                let remote_entity = RemoteEntity::new(remote_entity);
                self.remote
                    .deliver_message(id, msg.with_entity(remote_entity));
            }
        }
    }

    pub fn update_sent_command_entity_refs(
        &mut self,
        _global_entity: &GlobalEntity,
        old_entity: OwnedLocalEntity,
        new_entity: OwnedLocalEntity,
    ) {
        // Iterate through sent_command_packets and update entity references
        for (_, (_, commands)) in self.sent_command_packets.iter_mut() {
            for (_, message) in commands.iter_mut() {
                if let Some(entity) = message.entity() {
                    if entity == old_entity {
                        *message = message.clone().with_entity(new_entity);
                    }
                }
            }
        }
    }

    pub fn extract_host_entity_commands(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Vec<EntityCommand> {
        // Get host_entity from entity_map
        let host_entity = self
            .entity_map
            .global_entity_to_host_entity(global_entity)
            .unwrap();
        // Extract commands from host engine
        self.host.extract_entity_commands(&host_entity)
    }

    pub fn extract_host_component_kinds(
        &self,
        global_entity: &GlobalEntity,
    ) -> HashSet<ComponentKind> {
        // Get host_entity from entity_map
        let host_entity = self
            .entity_map
            .global_entity_to_host_entity(global_entity)
            .unwrap();
        // Get host_entity_channel from host engine
        let channel = self.host.get_entity_channel(&host_entity).unwrap();
        // Return component_channels clone
        channel.component_kinds().clone()
    }

    pub fn remove_host_entity(&mut self, global_entity: &GlobalEntity) {
        // Lookup host_entity FIRST before removing from entity_map
        let host_entity = self
            .entity_map
            .global_entity_to_host_entity(global_entity)
            .unwrap();
        // Remove from host engine
        self.host.remove_entity_channel(&host_entity);
        // Remove from entity_map LAST
        self.entity_map.remove_by_global_entity(global_entity);
    }

    pub fn insert_remote_entity(
        &mut self,
        global_entity: &GlobalEntity,
        remote_entity: RemoteEntity,
        component_kinds: HashSet<ComponentKind>,
    ) {
        // Insert into entity_map
        self.entity_map
            .insert_with_remote_entity(*global_entity, remote_entity);

        if self.remote.has_entity_channel(&remote_entity) {
            // Case: Channel was auto-created by messages arriving before the MigrateResponse event was processed
            // We need to upgrade this channel to be delegated and have the correct component state
            info!(
                "RemoteEntity({:?}) channel already exists (likely from out-of-order SetAuthority). Upgrading to Delegated.",
                remote_entity
            );
            let channel = self.remote.get_entity_channel_mut(&remote_entity).unwrap();

            // Upgrade to delegated
            channel.configure_as_delegated();

            // Set state to Spawned (if not already)
            // Note: We don't want to overwrite if it's already Spawned, but for migration we assume it should be
            channel.set_spawned(0);

            // Insert component channels
            for component_kind in component_kinds {
                channel.insert_component_channel_as_inserted(component_kind, 0);
            }
        } else {
            // Normal Case: Create new delegated channel
            let mut channel = RemoteEntityChannel::new_delegated(self.entity_map.host_type());

            // Set state to Spawned
            channel.set_spawned(0);

            // For each component_kind, add RemoteComponentChannel with inserted=true
            for component_kind in component_kinds {
                channel.insert_component_channel_as_inserted(component_kind, 0);
            }

            // Insert into remote engine
            self.remote.insert_entity_channel(remote_entity, channel);
        }
    }

    pub fn install_entity_redirect(&mut self, old: OwnedLocalEntity, new: OwnedLocalEntity) {
        self.entity_map.install_entity_redirect(old, new);
    }

    pub fn apply_entity_redirect(&self, entity: OwnedLocalEntity) -> OwnedLocalEntity {
        self.entity_map.apply_entity_redirect(&entity)
    }

    pub fn replay_entity_command(&mut self, global_entity: &GlobalEntity, command: EntityCommand) {
        // Send command through appropriate channel (should be remote after migration)
        let _remote_entity = self
            .entity_map
            .global_entity_to_remote_entity(global_entity)
            .unwrap();
        self.remote.send_entity_command(&self.entity_map, command);
    }
}

impl PacketNotifiable for LocalWorldManager {
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        self.host_notify_packet_delivered(packet_index);
        self.updater.notify_packet_delivered(packet_index);
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use crate::LocalEntity;

        impl LocalWorldManager {

            pub fn local_entities(&self) -> Vec<LocalEntity> {
                self.entity_map
                .iter()
                .map(|(_, record)| LocalEntity::from(record.owned_entity()))
                .collect::<Vec<LocalEntity>>()
            }
        }
    }
}

#[cfg(feature = "test_utils")]
impl LocalWorldManager {
    pub fn diff_handler_receiver_count(&self) -> usize {
        self.updater.diff_handler_receiver_count()
    }
}
