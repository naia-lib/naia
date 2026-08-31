use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use crate::{
    messages::channels::receivers::reliable_receiver::ReliableReceiver,
    world::{
        sync::{HostEngine, HostEntityChannel, RemoteEngine, RemoteEntityChannel},
        update::entity_update_manager::EntityUpdateManager,
    },
    ComponentKind, ComponentKinds, EntityCommand, EntityEvent, EntityMapConverterMut,
    EntityMessage, EntityMessageReceiver, EntityMessageType, GlobalEntity, GlobalEntitySpawner,
    GlobalWorldManagerType, HostEntity, HostEntityGenerator, HostType,
    LocalEntityAndGlobalEntityConverter, LocalEntityMap, MessageIndex, ShortMessageIndex,
    WorldMutType,
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

    // MISSION_SNAPSHOT_DIRTY_TRIM (2026-05-20): entities with a
    // value-reading outbound command (Spawn / SpawnWithComponents /
    // InsertComponent) that is not yet fully delivered to this peer.
    //
    // These are the entities whose component VALUES must remain in the
    // Sim→Send `SnapshotWorld` handoff: a reliable command re-reads the
    // current world value on every (re)transmit (`world_writer.rs`
    // SpawnWithComponents / InsertComponent), so dropping such an entity
    // from the snapshot would write a terminal `Noop` and silently lose
    // the spawn. Inserted when the command is queued; removed in
    // `process_delivered_commands` once the delivered engine has caught up
    // (entity present + all host component kinds delivered) or the host
    // channel is gone. Component value UPDATES are NOT tracked here — they
    // are covered by the cross-thread `GlobalDirtyBitset`.
    pending_outbound: HashSet<GlobalEntity>,

    // Per-entity set of component kinds CONFIRMED delivered to this peer,
    // maintained from acked Insert/Remove deliveries in
    // `process_delivered_commands`. Unlike the delivered `RemoteEntityChannel`'s
    // `component_channels` map (which keeps a kind's entry after a delivered
    // RemoveComponent), this set honors removes, so `host_entity_fully_delivered`
    // never sees a stale "delivered" kind after a remove → re-insert. See that
    // method for the full rationale (premature `pending_outbound` retire →
    // dirty-trim under-supply).
    delivered_component_kinds: HashMap<GlobalEntity, HashSet<ComponentKind>>,
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
            pending_outbound: HashSet::new(),
            delivered_component_kinds: HashMap::new(),
        }
    }

    /// Iterator over entities with an in-flight (not-yet-fully-delivered)
    /// value-reading command. See the `pending_outbound` field docs for
    /// why these must stay in the `SnapshotWorld` handoff.
    pub fn pending_outbound_entities(&self) -> impl Iterator<Item = GlobalEntity> + '_ {
        self.pending_outbound.iter().copied()
    }

    /// True iff `host_entity`'s spawn + every host-known component kind
    /// has been confirmed delivered (or the host channel no longer
    /// exists). Used to retire entries from `pending_outbound`.
    fn host_entity_fully_delivered(
        &self,
        host_entity: &HostEntity,
        global_entity: &GlobalEntity,
    ) -> bool {
        let Some(host_channel) = self.host_engine.get_entity_channel(host_entity) else {
            // Host channel gone (despawned / migrated) — nothing left to
            // (re)transmit a value for.
            return true;
        };
        // Spawn-delivery gate: the entity is not in the delivered world until
        // its Spawn / SpawnWithComponents has been acked. Until then it is
        // never fully delivered (covers the zero-component Spawn case, which
        // would otherwise vacuously pass the empty component check below).
        if self.get_delivered_world().get(host_entity).is_none() {
            return false;
        }
        // Component delivery: compare the host channel's CURRENT outstanding
        // kinds against the per-entity DELIVERED kind set. The delivered set is
        // maintained from acked Insert/Remove deliveries (`on_delivered_*_
        // component`), so it correctly reflects a remove. The delivered
        // `RemoteEntityChannel`'s `has_component_kind` must NOT be used here: its
        // `component_channels` map retains a kind's entry after a delivered
        // RemoveComponent (it is keyed for message-ordering, not presence), so it
        // reports a stale `true` for a removed component. On a remove → re-insert
        // of a RETAINED carrier (e.g. a replicated-resource carrier whose entity
        // registration survives `remove_replicated_resource`) that stale `true`
        // would mark the freshly re-added InsertComponent "already delivered" and
        // retire the entity from `pending_outbound` one tick after re-insert —
        // before the new value is actually acked. The dirty-trim snapshot would
        // then drop the entity while the reliable InsertComponent is still being
        // retransmitted, producing a `world_writer` needed-set under-supply
        // panic (silent insert loss in release). Over-retention is harmless by
        // the `pending_outbound` contract; premature retire is the bug this
        // guards against.
        let delivered = self.delivered_component_kinds.get(global_entity);
        host_channel
            .component_kinds()
            .iter()
            .all(|k| delivered.map(|set| set.contains(k)).unwrap_or(false))
    }

    /// L3 send-state seam variant: build the converter holding a write guard on
    /// the shared entity map (the map now lives behind `Arc<RwLock<..>>`).
    pub(crate) fn entity_converter_mut_guarded<'a, 'b>(
        &'b mut self,
        global_world_manager: &'a dyn GlobalWorldManagerType,
        entity_map_guard: std::sync::RwLockWriteGuard<'b, LocalEntityMap>,
    ) -> EntityMapConverterMut<'a, 'b> {
        EntityMapConverterMut::new(
            global_world_manager,
            entity_map_guard,
            &mut self.entity_generator,
        )
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
        // Either path queues a value-reading command → must stay in the snapshot until acked.
        self.pending_outbound.insert(*global_entity);
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

        // Either path queues a value-reading command → must stay in the snapshot until acked.
        self.pending_outbound.insert(*global_entity);
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
        // Value-reading commands (Spawn / SpawnWithComponents /
        // InsertComponent) re-read the world on (re)transmit, so the
        // entity must stay in the snapshot until acked. Despawn / auth /
        // remove commands carry no component payload — no snapshot need.
        match command.get_type() {
            EntityMessageType::Spawn
            | EntityMessageType::SpawnWithComponents
            | EntityMessageType::InsertComponent => {
                self.pending_outbound.insert(command.entity());
            }
            _ => {}
        }
        self.host_engine.send_command(converter, command);
    }

    /// Reserves an auth-channel command (`SubCommandId=0`) on the
    /// host entity's `HostEntityChannel`. See
    /// [`crate::world::sync::HostEntityChannel::reserve_first_command`].
    pub fn reserve_first_command(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        command: EntityCommand,
    ) {
        self.host_engine.reserve_first_command(converter, command);
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

        // MISSION_SNAPSHOT_DIRTY_TRIM: retire entities from `pending_outbound`
        // whose spawn + all host component kinds are now delivered (or whose
        // host channel is gone). This is the only removal site; it runs every
        // recv cycle. Over-retention (an entry lingering until the next cycle)
        // is harmless — it only keeps an already-delivered entity in the
        // snapshot a little longer. Under-removal is never a correctness bug;
        // missing an INSERT would be, which is why inserts hook every
        // value-reading command-send site.
        if !self.pending_outbound.is_empty() {
            let settled: Vec<GlobalEntity> = self
                .pending_outbound
                .iter()
                .copied()
                .filter(
                    |ge| match local_entity_map.global_entity_to_host_entity(ge) {
                        Ok(host_entity) => self.host_entity_fully_delivered(&host_entity, ge),
                        // No host mapping → entity is gone; stop tracking it.
                        Err(_) => true,
                    },
                )
                .collect();
            for ge in settled {
                self.pending_outbound.remove(&ge);
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
                EntityMessage::Despawn(host_entity) => {
                    // A client with Granted authority sent a Despawn for a server-created entity.
                    if let Some(global_entity) =
                        local_entity_map.global_entity_from_host(&host_entity)
                    {
                        self.incoming_events
                            .push(EntityEvent::Despawn(*global_entity));
                    }
                }
                EntityMessage::InsertComponent(_, _) => {
                    unreachable!(
                        "Server never sends InsertComponent to the originating HostWorldManager"
                    );
                }
                EntityMessage::RemoveComponent(_, _) => {
                    unreachable!(
                        "Server never sends RemoveComponent to the originating HostWorldManager"
                    );
                }
                EntityMessage::Publish(_, _) => {
                    unreachable!("Server never sends Publish to the originating HostWorldManager");
                }
                EntityMessage::Unpublish(_, _) => {
                    unreachable!(
                        "Server never sends Unpublish to the originating HostWorldManager"
                    );
                }
                EntityMessage::EnableDelegation(_, _) => {
                    unreachable!(
                        "Server never sends EnableDelegation to the originating HostWorldManager"
                    );
                }
                EntityMessage::DisableDelegation(_, _) => {
                    unreachable!(
                        "Server never sends DisableDelegation to the originating HostWorldManager"
                    );
                }
                EntityMessage::SetAuthority(_, _, _) => {
                    unreachable!(
                        "Server never sends SetAuthority to the originating HostWorldManager"
                    );
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
        if let Some(global_entity) = local_entity_map.global_entity_from_host(host_entity) {
            self.delivered_component_kinds.remove(global_entity);
        }
        self.entity_generator
            .remove_by_host_entity(local_entity_map, host_entity);
    }

    fn on_delivered_insert_component(
        &mut self,
        entity_update_manager: &mut EntityUpdateManager,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        // Component is already registered when entity comes into scope (in host_init_entity),
        // so we don't need to register again here when InsertComponent is delivered.
        // Mark the receiver delivered so Phase 3 can skip the 6+ HashMap lookup chain
        // of is_component_updatable_for_entity and use the single-lookup fast path instead.
        entity_update_manager.mark_component_delivered(global_entity, component_kind);
        self.delivered_component_kinds
            .entry(*global_entity)
            .or_default()
            .insert(*component_kind);
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
        if let Some(set) = self.delivered_component_kinds.get_mut(global_entity) {
            set.remove(component_kind);
        }
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

#[cfg(test)]
mod tests {
    //! Unit coverage for the host-side world manager.
    //!
    //! A sweep found 36 of 53 mutants surviving here: almost every delegating
    //! accessor could be replaced with a constant, every `process_delivered_
    //! commands` match arm deleted, and both `!` guards inverted, without a
    //! single test noticing. The integration suites drive this type only
    //! end-to-end, where a neutered accessor is masked by the layer above.

    use std::net::SocketAddr;

    use super::*;
    use crate::{
        bigmap::BigMapKey,
        world::{component::property::Property, test_support::TestGwm},
        ComponentFieldUpdate, EntityAndGlobalEntityConverter, GlobalEntityMap,
        PendingComponentUpdate, RemoteEntity, ReplicaDynMutWrapper,
        ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, Replicate, ReplicatedComponent,
        SerdeErr, WorldRefType,
    };

    #[derive(Replicate)]
    struct Ghost {
        value: Property<u8>,
    }

    /// Immutable components are never diff-tracked -- the guard in
    /// `init_entity_send_host_commands` exists to skip exactly this.
    #[derive(Replicate)]
    #[replicate(immutable)]
    struct Stone {
        value: Property<u8>,
    }

    // -- test doubles ------------------------------------------------------

    // -- fixture -----------------------------------------------------------

    struct Fixture {
        gwm: TestGwm,
        kinds: ComponentKinds,
        manager: HostWorldManager,
        updater: EntityUpdateManager,
    }

    fn ghost() -> ComponentKind {
        ComponentKind::of::<Ghost>()
    }

    fn stone() -> ComponentKind {
        ComponentKind::of::<Stone>()
    }

    /// Maps `GlobalEntity(id)` to `HostEntity(id)` and returns the pair.
    fn mapped(map: &mut LocalEntityMap, id: u32) -> (GlobalEntity, HostEntity) {
        let global_entity = GlobalEntity::from_u64(id as u64);
        let host_entity = HostEntity::new(id);
        map.insert_with_host_entity(global_entity, host_entity);
        (global_entity, host_entity)
    }

    impl Fixture {
        fn new() -> Self {
            let mut kinds = ComponentKinds::new();
            kinds.add_component::<Ghost>();
            kinds.add_component::<Stone>();

            let gwm = TestGwm::new(&kinds);
            let addr: Option<SocketAddr> = Some("127.0.0.1:4000".parse().unwrap());
            let updater = EntityUpdateManager::new(&addr, &gwm);

            Self {
                gwm,
                kinds,
                manager: HostWorldManager::new(HostType::Server, 1),
                updater,
            }
        }

        /// Gives the global diff handler a live receiver for `(entity, kind)`,
        /// so a later `register_component` on the ledger has something to find.
        fn arm_diff_handler(&self, entity: &GlobalEntity, kind: &ComponentKind) {
            self.gwm.arm_diff_handler(&self.kinds, entity, kind);
        }
    }

    // -- an inert world -----------------------------------------------------
    //
    // `process_incoming_messages` ignores its world and spawner entirely (both
    // parameters are `_`-prefixed), so every method here is `unreachable!`:
    // if the inbound path ever starts touching the world, these fire rather
    // than silently passing.

    struct InertWorld;

    impl WorldRefType<u64> for InertWorld {
        fn has_entity(&self, _: &u64) -> bool {
            unreachable!("the inbound path must not read the world")
        }
        fn entities(&self) -> Vec<u64> {
            unreachable!("the inbound path must not read the world")
        }
        fn has_component<R: ReplicatedComponent>(&self, _: &u64) -> bool {
            unreachable!("the inbound path must not read the world")
        }
        fn has_component_of_kind(&self, _: &u64, _: &ComponentKind) -> bool {
            unreachable!("the inbound path must not read the world")
        }
        fn component<'a, R: ReplicatedComponent>(
            &'a self,
            _: &u64,
        ) -> Option<ReplicaRefWrapper<'a, R>> {
            unreachable!("the inbound path must not read the world")
        }
        fn component_of_kind<'a>(
            &'a self,
            _: &u64,
            _: &ComponentKind,
        ) -> Option<ReplicaDynRefWrapper<'a>> {
            unreachable!("the inbound path must not read the world")
        }
    }

    impl WorldMutType<u64> for InertWorld {
        fn spawn_entity(&mut self) -> u64 {
            unreachable!("the inbound path must not mutate the world")
        }
        fn local_duplicate_entity(&mut self, _: &u64) -> u64 {
            unreachable!("the inbound path must not mutate the world")
        }
        fn local_duplicate_components(&mut self, _: &u64, _: &u64) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn despawn_entity(&mut self, _: &u64) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn component_kinds(&mut self, _: &u64) -> Vec<ComponentKind> {
            unreachable!("the inbound path must not mutate the world")
        }
        fn component_mut<'a, R: ReplicatedComponent>(
            &'a mut self,
            _: &u64,
        ) -> Option<ReplicaMutWrapper<'a, R>> {
            unreachable!("the inbound path must not mutate the world")
        }
        fn component_mut_of_kind<'a>(
            &'a mut self,
            _: &u64,
            _: &ComponentKind,
        ) -> Option<ReplicaDynMutWrapper<'a>> {
            unreachable!("the inbound path must not mutate the world")
        }
        fn component_apply_update(
            &mut self,
            _: &dyn LocalEntityAndGlobalEntityConverter,
            _: &u64,
            _: &ComponentKind,
            _: PendingComponentUpdate,
        ) -> Result<(), SerdeErr> {
            unreachable!("the inbound path must not mutate the world")
        }
        fn component_apply_field_update(
            &mut self,
            _: &dyn LocalEntityAndGlobalEntityConverter,
            _: &u64,
            _: &ComponentKind,
            _: ComponentFieldUpdate,
        ) -> Result<(), SerdeErr> {
            unreachable!("the inbound path must not mutate the world")
        }
        fn mirror_entities(&mut self, _: &u64, _: &u64) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn mirror_components(&mut self, _: &u64, _: &u64, _: &ComponentKind) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn insert_component<R: ReplicatedComponent>(&mut self, _: &u64, _: R) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn insert_boxed_component(&mut self, _: &u64, _: Box<dyn Replicate>) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn remove_component<R: ReplicatedComponent>(&mut self, _: &u64) -> Option<R> {
            unreachable!("the inbound path must not mutate the world")
        }
        fn remove_component_of_kind(
            &mut self,
            _: &u64,
            _: &ComponentKind,
        ) -> Option<Box<dyn Replicate>> {
            unreachable!("the inbound path must not mutate the world")
        }
        fn entity_publish(
            &mut self,
            _: &ComponentKinds,
            _: &dyn EntityAndGlobalEntityConverter<u64>,
            _: &dyn GlobalWorldManagerType,
            _: &u64,
        ) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn component_publish(
            &mut self,
            _: &ComponentKinds,
            _: &dyn EntityAndGlobalEntityConverter<u64>,
            _: &dyn GlobalWorldManagerType,
            _: &u64,
            _: &ComponentKind,
        ) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn entity_unpublish(&mut self, _: &u64) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn component_unpublish(&mut self, _: &u64, _: &ComponentKind) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn entity_enable_delegation(
            &mut self,
            _: &ComponentKinds,
            _: &dyn EntityAndGlobalEntityConverter<u64>,
            _: &dyn GlobalWorldManagerType,
            _: &u64,
        ) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn component_enable_delegation(
            &mut self,
            _: &ComponentKinds,
            _: &dyn EntityAndGlobalEntityConverter<u64>,
            _: &dyn GlobalWorldManagerType,
            _: &u64,
            _: &ComponentKind,
        ) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn entity_disable_delegation(&mut self, _: &u64) {
            unreachable!("the inbound path must not mutate the world")
        }
        fn component_disable_delegation(&mut self, _: &u64, _: &ComponentKind) {
            unreachable!("the inbound path must not mutate the world")
        }
    }

    // =====================================================================
    // Outbound command entry points
    // =====================================================================

    /// The zero-component path sends a plain `Spawn`; the populated path
    /// coalesces into a single `SpawnWithComponents`. Inverting the guard
    /// swaps them, which drops every initial component payload.
    #[test]
    fn init_entity_picks_the_spawn_shape_from_the_component_list() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (bare, _) = mapped(&mut map, 1);
        let (loaded, _) = mapped(&mut map, 2);

        fx.manager.init_entity_send_host_commands(
            map.entity_converter(),
            &bare,
            vec![],
            &mut fx.updater,
            &fx.kinds,
        );
        fx.manager.init_entity_send_host_commands(
            map.entity_converter(),
            &loaded,
            vec![ghost()],
            &mut fx.updater,
            &fx.kinds,
        );

        let commands = fx.manager.take_outgoing_commands();
        assert!(
            commands.contains(&EntityCommand::Spawn(bare)),
            "the zero-component path did not send a plain Spawn: {commands:?}",
        );
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, EntityCommand::SpawnWithComponents(e, k)
                    if *e == loaded && k == &vec![ghost()])),
            "the populated path did not coalesce into SpawnWithComponents: {commands:?}",
        );
    }

    /// Only mutable components are registered for diff-tracking. Deleting the
    /// `!` registers immutable components too, which permanently pins dirty
    /// state for a component that can never change.
    #[test]
    fn init_entity_diff_tracks_mutable_components_only() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, _) = mapped(&mut map, 1);
        fx.arm_diff_handler(&global_entity, &ghost());
        fx.arm_diff_handler(&global_entity, &stone());

        fx.manager.init_entity_send_host_commands(
            map.entity_converter(),
            &global_entity,
            vec![ghost(), stone()],
            &mut fx.updater,
            &fx.kinds,
        );

        assert!(
            fx.updater
                .diff_handler_has_component(&global_entity, &ghost()),
            "the mutable component was not registered for diff-tracking",
        );
        assert!(
            !fx.updater
                .diff_handler_has_component(&global_entity, &stone()),
            "an immutable component was registered for diff-tracking",
        );
    }

    /// The static path never diff-tracks, but it does pick the same two spawn
    /// shapes, and it must still hold the entity in the snapshot handoff.
    #[test]
    fn init_static_entity_picks_the_spawn_shape_and_holds_the_entity() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (bare, _) = mapped(&mut map, 1);
        let (loaded, _) = mapped(&mut map, 2);

        fx.manager
            .init_static_entity_send_host_commands(map.entity_converter(), &bare, vec![]);
        fx.manager.init_static_entity_send_host_commands(
            map.entity_converter(),
            &loaded,
            vec![stone()],
        );

        let commands = fx.manager.take_outgoing_commands();
        assert!(
            commands.contains(&EntityCommand::Spawn(bare)),
            "{commands:?}"
        );
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, EntityCommand::SpawnWithComponents(e, _) if *e == loaded)),
            "{commands:?}",
        );

        let pending: HashSet<GlobalEntity> = fx.manager.pending_outbound_entities().collect();
        assert!(pending.contains(&bare));
        assert!(pending.contains(&loaded));
    }

    /// `pending_outbound` is the snapshot-handoff hold list: only commands
    /// that RE-READ world values on retransmit may put an entity on it.
    /// Over-holding is harmless; under-holding silently loses a spawn.
    #[test]
    fn only_value_reading_commands_hold_an_entity_in_the_snapshot() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (held, _) = mapped(&mut map, 1);
        let (unheld, _) = mapped(&mut map, 2);

        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Spawn(held));
        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Spawn(unheld));
        let _ = fx.manager.take_outgoing_commands();
        // Clear the holds the two spawns just created.
        for ge in [held, unheld] {
            fx.manager.pending_outbound.remove(&ge);
        }

        fx.manager.send_command(
            map.entity_converter(),
            EntityCommand::InsertComponent(held, ghost()),
        );
        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Despawn(unheld));

        let pending: HashSet<GlobalEntity> = fx.manager.pending_outbound_entities().collect();
        assert!(
            pending.contains(&held),
            "an InsertComponent did not hold its entity in the snapshot",
        );
        assert!(
            !pending.contains(&unheld),
            "a Despawn carries no value payload and must not hold",
        );
    }

    /// A fresh manager holds nothing -- the anti-vacuity twin of the above.
    #[test]
    fn a_fresh_manager_holds_no_entities_in_the_snapshot() {
        let fx = Fixture::new();
        let _map = LocalEntityMap::new(HostType::Server);

        assert_eq!(fx.manager.pending_outbound_entities().count(), 0);
    }

    /// The outgoing drain is a hand-off, not a copy.
    #[test]
    fn taking_outgoing_commands_hands_them_over() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, _) = mapped(&mut map, 1);
        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Spawn(global_entity));

        assert!(!fx.manager.take_outgoing_commands().is_empty());
        assert!(
            fx.manager.take_outgoing_commands().is_empty(),
            "the take left the commands behind",
        );
    }

    /// The reservation must reach the outgoing buffer without waiting for a
    /// following `send_command`.
    #[test]
    fn a_reserved_first_command_reaches_the_outgoing_buffer() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, _) = mapped(&mut map, 1);
        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Spawn(global_entity));
        let _ = fx.manager.take_outgoing_commands();

        fx.manager.reserve_first_command(
            map.entity_converter(),
            EntityCommand::EnableDelegation(Some(0), global_entity),
        );

        assert!(
            !fx.manager.take_outgoing_commands().is_empty(),
            "the reserved command was stranded in the entity channel",
        );
    }

    /// Per-entity extraction is a hand-off too, and an untracked entity
    /// yields nothing rather than panicking.
    #[test]
    fn extracting_an_entitys_commands_empties_its_channel() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Spawn(global_entity));
        let _ = fx.manager.take_outgoing_commands();
        fx.manager
            .get_entity_channel_mut(&host_entity)
            .expect("fixture: the spawned entity should have a channel")
            .send_command(EntityCommand::EnableDelegation(Some(0), global_entity));

        let first = fx.manager.extract_entity_commands(&host_entity);
        let second = fx.manager.extract_entity_commands(&host_entity);

        assert!(!first.is_empty(), "the queued command was not extracted");
        assert!(second.is_empty(), "the extract left the command behind");
        assert!(fx
            .manager
            .extract_entity_commands(&HostEntity::new(99))
            .is_empty());
    }

    // =====================================================================
    // Registry views
    // =====================================================================

    /// Both directions, so neither constant answer survives.
    #[test]
    fn the_manager_reports_only_the_host_entities_it_tracks() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        assert!(!fx.manager.has_entity(&host_entity));

        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Spawn(global_entity));

        assert!(fx.manager.has_entity(&host_entity));
        assert!(!fx.manager.has_entity(&HostEntity::new(99)));
    }

    #[test]
    fn the_channel_lookups_find_only_tracked_entities() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Spawn(global_entity));

        assert!(fx.manager.get_entity_channel(&host_entity).is_some());
        assert!(fx
            .manager
            .get_entity_channel(&HostEntity::new(99))
            .is_none());
        assert!(fx.manager.get_entity_channel_mut(&host_entity).is_some());
        assert!(fx
            .manager
            .get_entity_channel_mut(&HostEntity::new(99))
            .is_none());
    }

    /// The migration path inserts a channel built elsewhere; it has to become
    /// the manager's own.
    #[test]
    fn an_inserted_entity_channel_becomes_the_managers_own() {
        let mut fx = Fixture::new();
        let _map = LocalEntityMap::new(HostType::Server);
        let host_entity = HostEntity::new(7);
        assert!(fx.manager.get_entity_channel(&host_entity).is_none());

        fx.manager
            .insert_entity_channel(host_entity, HostEntityChannel::new(HostType::Server));

        assert!(
            fx.manager.get_entity_channel(&host_entity).is_some(),
            "the inserted channel was not registered",
        );
        assert!(fx.manager.has_entity(&host_entity));
    }

    #[test]
    fn a_removed_entity_channel_leaves_the_manager() {
        let mut fx = Fixture::new();
        let _map = LocalEntityMap::new(HostType::Server);
        let host_entity = HostEntity::new(7);
        fx.manager
            .insert_entity_channel(host_entity, HostEntityChannel::new(HostType::Server));

        let _channel = fx.manager.remove_entity_channel(&host_entity);

        assert!(!fx.manager.has_entity(&host_entity));
    }

    /// Reserved host entities are handed back exactly once.
    #[test]
    fn a_reserved_entity_is_returned_once_and_then_gone() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let global_entity = GlobalEntity::from_u64(1);
        let reserved = fx.manager.host_reserve_entity(&mut map, &global_entity);

        assert_eq!(
            fx.manager.host_removed_reserved_entity(&global_entity),
            Some(reserved),
            "the reservation was not returned",
        );
        assert_eq!(
            fx.manager.host_removed_reserved_entity(&global_entity),
            None,
            "the reservation survived its removal",
        );
        assert_eq!(
            fx.manager
                .host_removed_reserved_entity(&GlobalEntity::from_u64(99)),
            None,
            "an entity that was never reserved produced a reservation",
        );
    }

    // =====================================================================
    // Delivery tracking
    // =====================================================================

    /// The delivered world starts empty and gains the entity only once its
    /// spawn has been acked and processed.
    #[test]
    fn the_delivered_world_grows_only_from_processed_acks() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (_global_entity, host_entity) = mapped(&mut map, 1);
        assert!(
            fx.manager.get_delivered_world().is_empty(),
            "the delivered world did not start empty",
        );

        fx.manager
            .deliver_message(1, EntityMessage::Spawn(host_entity));
        assert!(
            fx.manager.get_delivered_world().is_empty(),
            "a buffered ack was applied before it was processed",
        );

        fx.manager
            .process_delivered_commands(&mut map, &mut fx.updater);

        assert!(
            fx.manager.get_delivered_world().contains_key(&host_entity),
            "the processed spawn ack never reached the delivered world",
        );
    }

    /// A delivered despawn recycles the host entity id -- the mapping must be
    /// gone from the entity map afterwards.
    #[test]
    fn a_delivered_despawn_releases_the_host_entity_mapping() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (_global_entity, host_entity) = mapped(&mut map, 1);

        fx.manager
            .deliver_message(1, EntityMessage::Spawn(host_entity));
        fx.manager
            .deliver_message(2, EntityMessage::Despawn(host_entity));

        assert!(map.contains_host_entity(&host_entity));

        fx.manager
            .process_delivered_commands(&mut map, &mut fx.updater);

        assert!(
            !map.contains_host_entity(&host_entity),
            "the delivered despawn did not release the host entity mapping",
        );
    }

    /// A delivered `RemoveComponent` deregisters the component from
    /// diff-tracking. Without the arm the component keeps being considered for
    /// updates after it is gone.
    #[test]
    fn a_delivered_remove_component_deregisters_the_component() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        fx.arm_diff_handler(&global_entity, &ghost());
        fx.updater.register_component(&global_entity, &ghost());
        assert!(
            fx.updater
                .diff_handler_has_component(&global_entity, &ghost()),
            "fixture: the component was never registered",
        );

        fx.manager
            .deliver_message(1, EntityMessage::Spawn(host_entity));
        fx.manager
            .deliver_message(2, EntityMessage::InsertComponent(host_entity, ghost()));
        fx.manager
            .deliver_message(3, EntityMessage::RemoveComponent(host_entity, ghost()));

        fx.manager
            .process_delivered_commands(&mut map, &mut fx.updater);

        assert!(
            !fx.updater
                .diff_handler_has_component(&global_entity, &ghost()),
            "the delivered remove did not deregister the component",
        );
    }

    /// The snapshot hold is retired only once the spawn AND every host
    /// component kind is confirmed delivered. This is the `pending_outbound`
    /// contract: premature retire drops a live value from the snapshot.
    #[test]
    fn the_snapshot_hold_retires_only_when_everything_is_delivered() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        fx.manager.init_entity_send_host_commands(
            map.entity_converter(),
            &global_entity,
            vec![ghost()],
            &mut fx.updater,
            &fx.kinds,
        );
        let _ = fx.manager.take_outgoing_commands();

        // Nothing acked yet.
        fx.manager
            .process_delivered_commands(&mut map, &mut fx.updater);
        assert!(
            fx.manager
                .pending_outbound_entities()
                .any(|e| e == global_entity),
            "the hold was retired before anything was delivered",
        );

        // Spawn acked, but the component is not yet confirmed.
        fx.manager
            .deliver_message(1, EntityMessage::Spawn(host_entity));
        fx.manager
            .process_delivered_commands(&mut map, &mut fx.updater);
        assert!(
            fx.manager
                .pending_outbound_entities()
                .any(|e| e == global_entity),
            "the hold was retired while a component was still undelivered",
        );

        // Component acked: now everything is delivered.
        fx.manager
            .deliver_message(2, EntityMessage::InsertComponent(host_entity, ghost()));
        fx.manager
            .process_delivered_commands(&mut map, &mut fx.updater);
        assert!(
            !fx.manager
                .pending_outbound_entities()
                .any(|e| e == global_entity),
            "the hold survived full delivery",
        );
    }

    /// An entity with no host mapping left is gone; its hold must be dropped
    /// rather than pinned forever.
    #[test]
    fn the_snapshot_hold_is_dropped_for_an_entity_with_no_host_mapping() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, _) = mapped(&mut map, 1);
        fx.manager.init_static_entity_send_host_commands(
            map.entity_converter(),
            &global_entity,
            vec![],
        );
        let _ = fx.manager.take_outgoing_commands();

        // A map that never knew this entity.
        let mut empty_map = LocalEntityMap::new(HostType::Server);
        fx.manager
            .process_delivered_commands(&mut empty_map, &mut fx.updater);

        assert_eq!(
            fx.manager.pending_outbound_entities().count(),
            0,
            "an unmapped entity stayed pinned in the snapshot handoff",
        );
    }

    /// `MigrateResponse` is tracked for delivery but must never be fed to the
    /// delivered engine, whose entity channel panics on it. The filter has to
    /// drop exactly that variant and keep everything else.
    #[test]
    fn a_delivered_migrate_response_is_filtered_out_without_dropping_the_rest() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (_global_entity, host_entity) = mapped(&mut map, 1);

        fx.manager.deliver_message(
            1,
            EntityMessage::MigrateResponse(0, host_entity, RemoteEntity::new(5)),
        );
        fx.manager
            .deliver_message(2, EntityMessage::Spawn(host_entity));

        fx.manager
            .process_delivered_commands(&mut map, &mut fx.updater);

        assert!(
            fx.manager.get_delivered_world().contains_key(&host_entity),
            "the filter swallowed the messages it was supposed to keep",
        );
    }

    // =====================================================================
    // Update eligibility
    // =====================================================================

    /// A component is updatable only when the host still holds the kind AND
    /// the peer has confirmed it. Every step of the chain is asserted, so no
    /// constant answer and no dropped guard survives.
    #[test]
    fn a_component_is_updatable_only_once_the_peer_has_it() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        let unmapped = GlobalEntity::from_u64(99);

        assert!(
            !fx.manager
                .is_component_updatable(map.entity_converter(), &unmapped, &ghost()),
            "an entity with no host mapping reported an updatable component",
        );
        assert!(
            !fx.manager
                .is_component_updatable(map.entity_converter(), &global_entity, &ghost()),
            "an entity with no host channel reported an updatable component",
        );

        fx.manager.send_command(
            map.entity_converter(),
            EntityCommand::SpawnWithComponents(global_entity, vec![ghost()]),
        );
        let _ = fx.manager.take_outgoing_commands();
        assert!(
            !fx.manager
                .is_component_updatable(map.entity_converter(), &global_entity, &ghost()),
            "a component the peer has not confirmed reported as updatable",
        );

        fx.manager
            .deliver_message(1, EntityMessage::Spawn(host_entity));
        fx.manager
            .deliver_message(2, EntityMessage::InsertComponent(host_entity, ghost()));
        fx.manager
            .process_delivered_commands(&mut map, &mut fx.updater);

        assert!(
            fx.manager
                .is_component_updatable(map.entity_converter(), &global_entity, &ghost()),
            "a fully delivered component was not updatable",
        );
        assert!(
            !fx.manager
                .is_component_updatable(map.entity_converter(), &global_entity, &stone()),
            "a kind the host channel does not hold reported as updatable",
        );
    }

    // -- the inbound path ---------------------------------------------------

    /// A `Despawn` sent back by the peer for a host-owned entity surfaces as a
    /// `Despawn` event carrying the *global* entity: the manager has to
    /// translate through the local entity map, not pass the host id through.
    #[test]
    fn a_received_despawn_surfaces_as_a_global_despawn_event() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Spawn(global_entity));
        let _ = fx.manager.take_outgoing_commands();

        let mut spawner = GlobalEntityMap::<u64>::new();
        let mut world = InertWorld;
        let events = fx.manager.take_incoming_events(
            &mut spawner,
            &fx.gwm,
            &map,
            &mut world,
            vec![(1, EntityMessage::Despawn(host_entity))],
        );

        assert_eq!(events.len(), 1, "expected exactly one despawn event");
        assert!(
            matches!(&events[0], EntityEvent::Despawn(e) if *e == global_entity),
            "the despawn did not name the global entity",
        );
    }

    /// A `MigrateResponse` hands the client the remote entity id the server
    /// picked, paired with the global entity behind the host id it named.
    #[test]
    fn a_received_migrate_response_surfaces_the_new_remote_entity() {
        let fx = Fixture::new();
        // Only a client ever receives a `MigrateResponse`; a server that gets
        // one is hearing it from a client with no business sending it.
        let mut manager = HostWorldManager::new(HostType::Client, 0);
        let mut map = LocalEntityMap::new(HostType::Client);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        manager.send_command(map.entity_converter(), EntityCommand::Spawn(global_entity));
        let _ = manager.take_outgoing_commands();
        let new_remote = RemoteEntity::new(7);

        let mut spawner = GlobalEntityMap::<u64>::new();
        let mut world = InertWorld;
        let events = manager.take_incoming_events(
            &mut spawner,
            &fx.gwm,
            &map,
            &mut world,
            vec![(
                1,
                EntityMessage::MigrateResponse(0, host_entity, new_remote),
            )],
        );

        assert_eq!(events.len(), 1, "expected exactly one migrate response");
        assert!(
            matches!(
                &events[0],
                EntityEvent::MigrateResponse(e, r) if *e == global_entity && *r == new_remote
            ),
            "the migrate response did not carry the global/remote pair",
        );
    }

    /// A whitelisted authority message falls through to `to_event`, so the
    /// event it produces reaches the caller unchanged.
    #[test]
    fn a_received_release_authority_falls_through_to_its_own_event() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        for command in [
            EntityCommand::Spawn(global_entity),
            EntityCommand::EnableDelegation(None, global_entity),
        ] {
            fx.manager.send_command(map.entity_converter(), command);
        }
        let _ = fx.manager.take_outgoing_commands();

        let mut spawner = GlobalEntityMap::<u64>::new();
        let mut world = InertWorld;
        let events = fx.manager.take_incoming_events(
            &mut spawner,
            &fx.gwm,
            &map,
            &mut world,
            vec![(1, EntityMessage::ReleaseAuthority(0, host_entity))],
        );

        assert_eq!(
            events.len(),
            1,
            "the whitelisted message did not fall through to its own event",
        );
        assert!(matches!(
            &events[0],
            EntityEvent::ReleaseAuthority(e) if *e == global_entity
        ));
    }

    /// A `Noop` is dropped without producing an event, and the take is a
    /// hand-off: a second call comes back empty.
    #[test]
    fn the_incoming_take_drops_noops_and_hands_over_the_buffer() {
        let mut fx = Fixture::new();
        let mut map = LocalEntityMap::new(HostType::Server);
        let (global_entity, host_entity) = mapped(&mut map, 1);
        fx.manager
            .send_command(map.entity_converter(), EntityCommand::Spawn(global_entity));
        let _ = fx.manager.take_outgoing_commands();

        let mut spawner = GlobalEntityMap::<u64>::new();
        let mut world = InertWorld;
        assert!(
            fx.manager
                .take_incoming_events(
                    &mut spawner,
                    &fx.gwm,
                    &map,
                    &mut world,
                    vec![(1, EntityMessage::Noop)],
                )
                .is_empty(),
            "a noop produced an event",
        );

        assert!(!fx
            .manager
            .take_incoming_events(
                &mut spawner,
                &fx.gwm,
                &map,
                &mut world,
                vec![(2, EntityMessage::Despawn(host_entity))],
            )
            .is_empty());
        assert!(
            fx.manager
                .take_incoming_events(&mut spawner, &fx.gwm, &map, &mut world, vec![])
                .is_empty(),
            "the take left the events behind",
        );
    }
}
