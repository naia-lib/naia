use std::{
    collections::HashMap,
    hash::Hash,
};

use crate::{
    messages::channels::receivers::reliable_receiver::ReliableReceiver,
    world::{
        sync::{HostEngine, HostEntityChannel, RemoteEngine, RemoteEntityChannel},
        update::entity_update_manager::EntityUpdateManager,
    },
    ComponentKind, ComponentKinds, EntityCommand, EntityConverterMut, EntityEvent, EntityMessage,
    EntityMessageReceiver, GlobalEntity, GlobalEntitySpawner, GlobalWorldManagerType, HostEntity,
    HostEntityGenerator, HostType, LocalEntityAndGlobalEntityConverter, LocalEntityMap,
    MessageIndex, ShortMessageIndex, WorldMutType,
};

/// Sequence number identifying a top-level entity command sent over the reliable channel.
pub type CommandId = MessageIndex;
/// Sequence number identifying a sub-command within a top-level entity command.
pub type SubCommandId = ShortMessageIndex;

/// Drives outbound entity-lifecycle replication for one side of a connection, tracking delivery state and processing inbound authority responses.
pub struct HostWorldManager {
    // host entity generator
    entity_generator: HostEntityGenerator,

    // For Server, this contains the Entities that the Server has authority over, that it syncs to the Client
    // For Client, this contains the non-Delegated Entities that the Client has authority over, that it syncs to the Server
    host_engine: HostEngine,

    // For Server, this contains the Entities that the Server has authority over, that have been delivered to the Client
    // For Client, this contains the non-Delegated Entities that the Client has authority over, that have been delivered to the Server
    delivered_receiver: ReliableReceiver<EntityMessage<HostEntity>>,
    delivered_engine: RemoteEngine<HostEntity>,
    incoming_events: Vec<EntityEvent>,
}

impl HostWorldManager {
    /// Creates a `HostWorldManager` for the given `host_type` side and `user_key`.
    pub fn new(host_type: HostType, user_key: u64) -> Self {
        Self {
            entity_generator: HostEntityGenerator::new(user_key),
            host_engine: HostEngine::new(host_type),
            delivered_receiver: ReliableReceiver::new(),
            delivered_engine: RemoteEngine::new(host_type.invert()),
            incoming_events: Vec::new(),
        }
    }

    pub(crate) fn entity_converter_mut<'a, 'b>(
        &'b mut self,
        global_world_manager: &'a dyn GlobalWorldManagerType,
        entity_map: &'b mut LocalEntityMap,
    ) -> EntityConverterMut<'a, 'b> {
        EntityConverterMut::new(global_world_manager, entity_map, &mut self.entity_generator)
    }

    // Collect

    /// Processes `incoming_messages` through the host engine and returns all resulting [`EntityEvent`]s.
    pub fn take_incoming_events<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        spawner: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        local_entity_map: &LocalEntityMap,
        world: &mut W,
        incoming_messages: Vec<(MessageIndex, EntityMessage<HostEntity>)>,
    ) -> Vec<EntityEvent> {
        let incoming_messages = EntityMessageReceiver::host_take_incoming_events(
            &mut self.host_engine,
            incoming_messages,
        );

        self.process_incoming_messages(
            spawner,
            global_world_manager,
            local_entity_map,
            world,
            incoming_messages,
        );

        std::mem::take(&mut self.incoming_events)
    }

    /// Drains and returns all pending outbound [`EntityCommand`]s queued by the host engine.
    pub fn take_outgoing_commands(&mut self) -> Vec<EntityCommand> {
        self.host_engine.take_outgoing_commands()
    }

    pub(crate) fn host_generate_entity(&mut self) -> HostEntity {
        self.entity_generator.generate_host_entity()
    }

    pub(crate) fn host_generate_static_entity(&mut self) -> HostEntity {
        self.entity_generator.generate_static_host_entity()
    }

    /// Sends the initial spawn command(s) for a static entity, coalescing components into a single message when present.
    pub fn init_static_entity_send_host_commands(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        global_entity: &GlobalEntity,
        component_kinds: Vec<ComponentKind>,
    ) {
        // Static entities: NEVER register for diff-tracking — they don't change after spawn.
        if !component_kinds.is_empty() {
            self.host_engine.send_command(
                converter,
                EntityCommand::SpawnWithComponents(*global_entity, component_kinds),
            );
            return;
        }
        self.host_engine
            .send_command(converter, EntityCommand::Spawn(*global_entity));
    }

    pub(crate) fn host_reserve_entity(
        &mut self,
        entity_map: &mut LocalEntityMap,
        global_entity: &GlobalEntity,
    ) -> HostEntity {
        self.entity_generator
            .host_reserve_entity(entity_map, global_entity)
    }

    pub(crate) fn host_removed_reserved_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Option<HostEntity> {
        self.entity_generator
            .host_remove_reserved_entity(global_entity)
    }

    pub(crate) fn has_entity(&self, host_entity: &HostEntity) -> bool {
        self.get_host_world().contains_key(host_entity)
    }

    /// Registers components for diff-tracking and sends initial spawn command(s) when an entity first enters connection scope.
    pub fn init_entity_send_host_commands(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        global_entity: &GlobalEntity,
        component_kinds: Vec<ComponentKind>,
        entity_update_manager: &mut EntityUpdateManager,
        component_kinds_map: &ComponentKinds,
    ) {
        // Register only mutable components for diff-tracking immediately at scope entry.
        // Immutable components (is_immutable == true) are never diff-tracked — skip them.
        for component_kind in &component_kinds {
            if !component_kinds_map.kind_is_immutable(component_kind) {
                entity_update_manager.register_component(global_entity, component_kind);
            }
        }

        if !component_kinds.is_empty() {
            // Coalesce Spawn + N InsertComponent into one reliable message
            self.host_engine.send_command(
                converter,
                EntityCommand::SpawnWithComponents(*global_entity, component_kinds),
            );
            return;
        }

        // Zero-component path: plain Spawn with no component payloads
        self.host_engine
            .send_command(converter, EntityCommand::Spawn(*global_entity));
    }

    /// Enqueues `command` for reliable delivery to the remote peer.
    pub fn send_command(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        command: EntityCommand,
    ) {
        self.host_engine.send_command(converter, command);
    }

    pub(crate) fn get_host_world(&self) -> &HashMap<HostEntity, HostEntityChannel> {
        self.host_engine.get_world()
    }

    pub(crate) fn extract_entity_commands(
        &mut self,
        host_entity: &HostEntity,
    ) -> Vec<EntityCommand> {
        self.host_engine.extract_entity_commands(host_entity)
    }

    pub(crate) fn get_delivered_world(&self) -> &HashMap<HostEntity, RemoteEntityChannel> {
        self.delivered_engine.get_world()
    }

    pub(crate) fn is_component_updatable(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        global_entity: &GlobalEntity,
        kind: &ComponentKind,
    ) -> bool {
        let Ok(host_entity) = converter.global_entity_to_host_entity(global_entity) else {
            return false;
        };
        let Some(host_channel) = self.get_host_world().get(&host_entity) else {
            return false;
        };
        if !host_channel.component_kinds().contains(kind) {
            return false;
        }
        let Some(delivered_channel) = self.get_delivered_world().get(&host_entity) else {
            return false;
        };
        delivered_channel.has_component_kind(kind)
    }

    pub(crate) fn deliver_message(
        &mut self,
        command_id: CommandId,
        message: EntityMessage<HostEntity>,
    ) {
        self.delivered_receiver.buffer_message(command_id, message);
    }

    pub(crate) fn process_delivered_commands(
        &mut self,
        local_entity_map: &mut LocalEntityMap,
        entity_update_manager: &mut EntityUpdateManager,
    ) {
        let delivered_messages: Vec<(MessageIndex, EntityMessage<HostEntity>)> =
            self.delivered_receiver.receive_messages();

        // Filter out MigrateResponse messages - they should not be processed by RemoteEngine
        // MigrateResponse is a client-only message that the server tracks for delivery but doesn't process
        let filtered_messages: Vec<(MessageIndex, EntityMessage<HostEntity>)> = delivered_messages
            .into_iter()
            .filter(|(_, msg)| !matches!(msg, EntityMessage::MigrateResponse(_, _, _)))
            .collect();

        for message in EntityMessageReceiver::remote_take_incoming_messages(
            &mut self.delivered_engine,
            filtered_messages,
        ) {
            match message {
                EntityMessage::Spawn(host_entity) => {
                    self.on_delivered_spawn_entity(&host_entity);
                }
                EntityMessage::Despawn(host_entity) => {
                    self.on_delivered_despawn_entity(local_entity_map, &host_entity);
                }
                EntityMessage::InsertComponent(host_entity, component_kind) => {
                    let Some(global_entity) =
                        local_entity_map.global_entity_from_host(&host_entity)
                    else {
                        return;
                    };
                    self.on_delivered_insert_component(
                        entity_update_manager,
                        global_entity,
                        &component_kind,
                    );
                }
                EntityMessage::RemoveComponent(host_entity, component_kind) => {
                    let Some(global_entity) =
                        local_entity_map.global_entity_from_host(&host_entity)
                    else {
                        return;
                    };
                    self.on_delivered_remove_component(
                        entity_update_manager,
                        global_entity,
                        &component_kind,
                    );
                }
                EntityMessage::Noop => {
                    // do nothing
                }
                _ => {
                    // Only Auth-related messages are left here
                    // Right now it doesn't seem like we need to track auth state here
                }
            }
        }
    }

    fn process_incoming_messages<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        _spawner: &mut dyn GlobalEntitySpawner<E>,
        _global_world_manager: &dyn GlobalWorldManagerType,
        local_entity_map: &LocalEntityMap,
        _world: &mut W,
        incoming_messages: Vec<EntityMessage<HostEntity>>,
    ) {
        // execute the action and emit an event
        for message in incoming_messages {
            match message {
                // These variants are sent server→client for remote-owned entities, routed through
                // RemoteWorldManager, not HostWorldManager. A HostWorldManager processes messages
                // about client-created (host-owned) entities only; the server never sends these
                // variants back to the originating host.
                EntityMessage::Spawn(_) => {
                    unreachable!("Server never sends Spawn to the originating HostWorldManager");
                }
                EntityMessage::Despawn(_) => {
                    unreachable!("Server never sends Despawn to the originating HostWorldManager");
                }
                EntityMessage::InsertComponent(_, _) => {
                    unreachable!("Server never sends InsertComponent to the originating HostWorldManager");
                }
                EntityMessage::RemoveComponent(_, _) => {
                    unreachable!("Server never sends RemoveComponent to the originating HostWorldManager");
                }
                EntityMessage::Publish(_, _) => {
                    unreachable!("Server never sends Publish to the originating HostWorldManager");
                }
                EntityMessage::Unpublish(_, _) => {
                    unreachable!("Server never sends Unpublish to the originating HostWorldManager");
                }
                EntityMessage::EnableDelegation(_, _) => {
                    unreachable!("Server never sends EnableDelegation to the originating HostWorldManager");
                }
                EntityMessage::DisableDelegation(_, _) => {
                    unreachable!("Server never sends DisableDelegation to the originating HostWorldManager");
                }
                EntityMessage::SetAuthority(_, _, _) => {
                    unreachable!("Server never sends SetAuthority to the originating HostWorldManager");
                }
                EntityMessage::MigrateResponse(_sub_id, client_host_entity, new_remote_entity) => {
                    // Client receives MigrateResponse from server telling it to migrate
                    // a client-created delegated entity from HostEntity to RemoteEntity

                    // Look up the global entity from the client's HostEntity
                    let global_entity = *local_entity_map.global_entity_from_host(&client_host_entity)
                        .expect("Host entity not found in local entity map during MigrateResponse processing");

                    // Create event for the client to process the migration
                    self.incoming_events.push(EntityEvent::MigrateResponse(
                        global_entity,
                        new_remote_entity,
                    ));
                }
                EntityMessage::Noop => {
                    // do nothing
                }
                // Whitelisted incoming messages:
                // 1. EntityMessage::EnableDelegationResponse
                // 2. EntityMessage::RequestAuthority
                // 3. EntityMessage::ReleaseAuthority
                msg => {
                    if let Some(event) = msg.to_event(local_entity_map) {
                        self.incoming_events.push(event);
                    }
                }
            }
        }
    }

    fn on_delivered_spawn_entity(&mut self, _host_entity: &HostEntity) {
        #[cfg(feature = "observability")]
        metrics::counter!(crate::SERVER_SPAWNS_TOTAL).increment(1);
    }

    /// Handles confirmed delivery of a despawn command, recycling the host entity ID and updating metrics.
    pub fn on_delivered_despawn_entity(
        &mut self,
        local_entity_map: &mut LocalEntityMap,
        host_entity: &HostEntity,
    ) {
        #[cfg(feature = "observability")]
        metrics::counter!(crate::SERVER_DESPAWNS_TOTAL).increment(1);
        self.entity_generator
            .remove_by_host_entity(local_entity_map, host_entity);
    }

    fn on_delivered_insert_component(
        &mut self,
        _entity_update_manager: &mut EntityUpdateManager,
        _global_entity: &GlobalEntity,
        _component_kind: &ComponentKind,
    ) {
        // Component is already registered when entity comes into scope (in host_init_entity),
        // so we don't need to register again here when InsertComponent is delivered
        #[cfg(feature = "observability")]
        metrics::counter!(crate::SERVER_COMPONENT_INSERTS_TOTAL).increment(1);
    }

    fn on_delivered_remove_component(
        &mut self,
        entity_update_manager: &mut EntityUpdateManager,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        #[cfg(feature = "observability")]
        metrics::counter!(crate::SERVER_COMPONENT_REMOVES_TOTAL).increment(1);
        entity_update_manager.deregister_component(global_entity, component_kind);
    }

    pub(crate) fn insert_entity_channel(&mut self, entity: HostEntity, channel: HostEntityChannel) {
        self.host_engine.insert_entity_channel(entity, channel);
    }

    pub(crate) fn get_entity_channel(&self, entity: &HostEntity) -> Option<&HostEntityChannel> {
        self.host_engine.get_entity_channel(entity)
    }

    pub(crate) fn get_entity_channel_mut(
        &mut self,
        entity: &HostEntity,
    ) -> Option<&mut HostEntityChannel> {
        self.host_engine.get_entity_channel_mut(entity)
    }

    pub(crate) fn remove_entity_channel(&mut self, entity: &HostEntity) -> HostEntityChannel {
        self.host_engine.remove_entity_channel(entity)
    }

}
// NOTE: on_delivered_migrate_response was removed (2026-05-10). The entity migration path
// requires RemoteWorldManager drain/extract/despawn APIs that do not exist. Any future
// implementation must correctly extract component_kinds and host_type from the remote channel
// before constructing the new HostEntityChannel — the prior stub silently passed wrong values.
