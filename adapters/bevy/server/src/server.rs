use std::time::Duration;

use bevy_ecs::{
    component::Component,
    entity::Entity,
    resource::Resource,
    system::{ResMut, SystemParam},
    world::{Mut, World},
};

use naia_server::{
    shared::SocketConfig, transport::Socket, ConnectionStats, EntityOwner, EntityPriorityMut,
    EntityPriorityRef, Events, Historian, NaiaServerError, ReplicationConfig, RoomKey, RoomMut,
    RoomRef, Server as NaiaServer, TickBufferMessages, TickEvents, UserKey, UserMut, UserRef,
    UserScopeMut, UserScopeRef, WorldServer as NaiaWorldServer,
};

use naia_bevy_shared::{
    Channel, ComponentKind, EntityAndGlobalEntityConverter, EntityAuthStatus,
    EntityDoesNotExistError, GlobalEntity, HostOwned, Instant, Message, ReplicatedResource, Request,
    Response, ResponseReceiveKey, ResponseSendKey, Tick, WorldMutType, WorldProxyMut, WorldRefType,
};

use crate::plugin::Singleton;

/// G6 — naia-owned outbox of sim-world resource carriers awaiting coord-side
/// registration. Lazily inserted into the sim subapp world by
/// [`Server::pipeline_replicate_resource`] and drained, against the coordinator,
/// by [`Server::pipeline_drain_resource_registrations`]. Owning this here (vs a
/// consumer-defined carrier) keeps pipelined-sim resource replication a naia
/// engine concern.
#[derive(Resource, Default)]
pub struct PendingResourceRegistrations(Vec<Entity>);

/// G5b — a sim-side room/authority op deferred to the park window. A pipelined
/// Sim world is naia-handle-free, so editor-style systems queue these intents
/// into [`PendingScopeAuthorityOps`] and the orchestrator drains them against the
/// parked coordinator via [`Server::pipeline_drain_scope_authority_ops`]. Owning
/// the op vocabulary + queue here (vs a consumer-defined enum) keeps pipelined-sim
/// room/authority deferral a naia engine concern.
#[derive(Debug, Clone)]
pub enum PipelineScopeAuthorityOp {
    /// Add a sim-world entity to a room (`WorldServer::room_add_entity`).
    RoomAdd(Entity),
    /// Reclaim server authority over an entity, breaking any client delegation
    /// lock (`WorldServer::entity_take_authority`).
    TakeAuthority(Entity),
    /// Delegate authority over an entity to a client
    /// (`WorldServer::entity_give_authority`).
    GiveAuthority(UserKey, Entity),
}

/// G5b — naia-owned outbox of [`PipelineScopeAuthorityOp`]s queued by sim-side
/// systems, drained against the coordinator by
/// [`Server::pipeline_drain_scope_authority_ops`]. Initialized into the sim
/// subapp world by the consumer (like any sim resource it pushes to) and owned
/// here so the op vocabulary stays a naia engine concern.
#[derive(Resource, Default)]
pub struct PendingScopeAuthorityOps(pub Vec<PipelineScopeAuthorityOp>);

use crate::Replicate;

#[derive(Resource)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum ServerImpl {
    Full(NaiaServer<Entity>),
    WorldOnly(NaiaWorldServer<Entity>),
}

impl ServerImpl {
    pub(crate) fn full(full_server: NaiaServer<Entity>) -> Self {
        Self::Full(full_server)
    }

    pub(crate) fn world_only(world_only_server: NaiaWorldServer<Entity>) -> Self {
        Self::WorldOnly(world_only_server)
    }

    pub(crate) fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        match self {
            Self::Full(server) => server.listen(socket),
            // The enum's `listen` performs the socket split + `io_load`
            // internally for its Resident arm — exactly what the old inline
            // body did here.
            Self::WorldOnly(server) => server.listen(socket),
        }
    }

    pub(crate) fn is_listening(&self) -> bool {
        match self {
            Self::Full(server) => server.is_listening(),
            Self::WorldOnly(server) => server.is_listening(),
        }
    }

    pub(crate) fn receive_all_packets(&mut self) {
        match self {
            Self::Full(server) => server.receive_all_packets(),
            Self::WorldOnly(server) => server.receive_all_packets(),
        }
    }

    pub(crate) fn process_all_packets<W: WorldMutType<Entity>>(
        &mut self,
        world: W,
        now: &Instant,
    ) {
        match self {
            // Both the `Server` and the unified `WorldServer` enum take the
            // world by value here. Byte-identical either way.
            Self::Full(server) => server.process_all_packets(world, now),
            Self::WorldOnly(server) => server.process_all_packets(world, now),
        }
    }

    pub(crate) fn take_world_events(&mut self) -> Events<Entity> {
        match self {
            Self::Full(server) => server.take_world_events(),
            Self::WorldOnly(server) => {
                let world_events = server.take_world_events();
                Events::<Entity>::from(world_events)
            }
        }
    }

    pub(crate) fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        match self {
            Self::Full(server) => server.take_tick_events(now),
            Self::WorldOnly(server) => server.take_tick_events(now),
        }
    }

    pub(crate) fn send_all_packets<W: WorldRefType<Entity> + Sync>(&mut self, world: W) {
        match self {
            Self::Full(server) => server.send_all_packets(world),
            Self::WorldOnly(server) => server.send_all_packets(world),
        }
    }

    pub(crate) fn entity_authority_status<W: WorldRefType<Entity>>(
        &self,
        world: W,
        entity: &Entity,
    ) -> Option<EntityAuthStatus> {
        if !world.has_entity(entity) {
            return None;
        }
        match self {
            Self::Full(server) => server.entity(world, entity).authority(),
            Self::WorldOnly(server) => server.entity_authority_status(entity),
        }
    }

    pub(crate) fn entity_owner<W: WorldRefType<Entity>>(
        &self,
        world: W,
        entity: &Entity,
    ) -> EntityOwner {
        match self {
            Self::Full(server) => server.entity(world, entity).owner(),
            Self::WorldOnly(server) => server.entity_owner(entity),
        }
    }

    pub(crate) fn configure_entity_replication<W: WorldMutType<Entity>>(
        &mut self,
        world: &mut W,
        world_entity: &Entity,
        config: ReplicationConfig,
    ) {
        match self {
            Self::Full(server) => {
                server.configure_entity_replication::<W>(world, world_entity, config)
            }
            Self::WorldOnly(server) => {
                server.configure_entity_replication::<W>(world, world_entity, config)
            }
        }
    }

    pub(crate) fn mark_entity_as_static(&mut self, entity: &Entity) {
        match self {
            Self::Full(server) => server.mark_entity_as_static(entity),
            Self::WorldOnly(server) => server.mark_entity_as_static(entity),
        }
    }

    pub(crate) fn insert_component_worldless(
        &mut self,
        entity: &Entity,
        component: &mut dyn Replicate,
    ) {
        match self {
            Self::Full(server) => server.insert_component_worldless(entity, component),
            Self::WorldOnly(server) => server.insert_component_worldless(entity, component),
        }
    }

    pub(crate) fn remove_component_worldless(
        &mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) {
        match self {
            Self::Full(server) => server.remove_component_worldless(entity, component_kind),
            Self::WorldOnly(server) => server.remove_component_worldless(entity, component_kind),
        }
    }

    pub(crate) fn despawn_entity_worldless(&mut self, entity: &Entity) {
        match self {
            Self::Full(server) => server.despawn_entity_worldless(entity),
            Self::WorldOnly(server) => server.despawn_entity_worldless(entity),
        }
    }

    // Replicated Resources -----------------------------------------------

    pub(crate) fn has_resource<R: ReplicatedResource>(&self) -> bool {
        match self {
            Self::Full(server) => server.has_resource::<R>(),
            Self::WorldOnly(server) => server.has_resource::<R>(),
        }
    }

    pub(crate) fn resource_entity<R: ReplicatedResource>(&self) -> Option<Entity> {
        match self {
            Self::Full(server) => server.resource_entity::<R>(),
            Self::WorldOnly(server) => server.resource_entity::<R>(),
        }
    }

    pub(crate) fn is_resource_entity(&self, entity: &Entity) -> bool {
        match self {
            Self::Full(server) => server.is_resource_entity(entity),
            Self::WorldOnly(server) => server.is_resource_entity(entity),
        }
    }

    pub(crate) fn resources_count(&self) -> usize {
        match self {
            Self::Full(server) => server.resources_count(),
            Self::WorldOnly(server) => server.resources_count(),
        }
    }

    pub(crate) fn insert_resource<W: WorldMutType<Entity>, R: ReplicatedResource>(
        &mut self,
        world: W,
        value: R,
        is_static: bool,
    ) -> Result<Entity, naia_bevy_shared::ResourceAlreadyExists> {
        match self {
            Self::Full(server) => server.insert_resource(world, value, is_static),
            Self::WorldOnly(server) => server.insert_resource(world, value, is_static),
        }
    }

    pub(crate) fn remove_resource<W: WorldMutType<Entity>, R: ReplicatedResource>(
        &mut self,
        world: W,
    ) -> bool {
        match self {
            Self::Full(server) => server.remove_resource::<W, R>(world),
            Self::WorldOnly(server) => server.remove_resource::<W, R>(world),
        }
    }

    pub(crate) fn configure_resource<W: WorldMutType<Entity>, R: ReplicatedResource>(
        &mut self,
        world: &mut W,
        config: ReplicationConfig,
    ) -> bool {
        match self {
            Self::Full(server) => server.configure_resource::<W, R>(world, config),
            Self::WorldOnly(server) => {
                let Some(entity) = server.resource_entity::<R>() else {
                    return false;
                };
                server.configure_entity_replication(world, &entity, config);
                true
            }
        }
    }

    pub(crate) fn resource_authority_status<R: ReplicatedResource>(
        &self,
    ) -> Option<EntityAuthStatus> {
        match self {
            Self::Full(server) => server.resource_authority_status::<R>(),
            Self::WorldOnly(server) => server.resource_authority_status::<R>(),
        }
    }

    /// Borrow the underlying [`WorldServer`] when this is the `WorldOnly` arm
    /// (the pipelined plugin's resource shape); `None` for `Full`.
    pub(crate) fn as_world_server(&self) -> Option<&NaiaWorldServer<Entity>> {
        match self {
            Self::WorldOnly(server) => Some(server),
            Self::Full(_) => None,
        }
    }

    /// Mutable variant of [`Self::as_world_server`].
    pub(crate) fn as_world_server_mut(&mut self) -> Option<&mut NaiaWorldServer<Entity>> {
        match self {
            Self::WorldOnly(server) => Some(server),
            Self::Full(_) => None,
        }
    }
}

// Server

#[derive(SystemParam)]
pub struct Server<'w> {
    server_impl: ResMut<'w, ServerImpl>,
}

impl<'w> Server<'w> {
    // Public Methods //

    //// Connections ////

    pub fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        self.server_impl.listen(socket);
    }

    pub fn is_listening(&self) -> bool {
        self.server_impl.is_listening()
    }

    pub fn accept_connection(&mut self, user_key: &UserKey) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(_server) => {
                panic!("WorldOnly Servers do not support this function")
            }
            ServerImpl::Full(server) => server.accept_connection(user_key),
        }
    }

    pub fn reject_connection(&mut self, user_key: &UserKey) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(_server) => {
                panic!("WorldOnly Servers do not support this function")
            }
            ServerImpl::Full(server) => server.reject_connection(user_key),
        }
    }

    // Config
    pub fn socket_config(&self) -> &SocketConfig {
        match &*self.server_impl {
            ServerImpl::WorldOnly(_server) => {
                panic!("WorldOnly Servers do not support this function")
            }
            ServerImpl::Full(server) => server.socket_config(),
        }
    }

    //// Messages ////
    pub fn send_message<C: Channel, M: Message>(
        &mut self,
        user_key: &UserKey,
        message: &M,
    ) -> Result<(), NaiaServerError> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.send_message::<C, M>(user_key, message),
            ServerImpl::Full(server) => server.send_message::<C, M>(user_key, message),
        }
    }

    /// Sends a message to all connected users using a given channel
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.broadcast_message::<C, M>(message),
            ServerImpl::Full(server) => server.broadcast_message::<C, M>(message),
        }
    }

    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.receive_tick_buffer_messages(tick),
            ServerImpl::Full(server) => server.receive_tick_buffer_messages(tick),
        }
    }

    /// Requests ///
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.send_request::<C, Q>(user_key, request),
            ServerImpl::Full(server) => server.send_request::<C, Q>(user_key, request),
        }
    }

    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.send_response(response_key, response),
            ServerImpl::Full(server) => server.send_response(response_key, response),
        }
    }

    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.receive_response(response_key),
            ServerImpl::Full(server) => server.receive_response(response_key),
        }
    }

    //// Updates ////

    pub fn scope_checks_pending(&self) -> Vec<(RoomKey, UserKey, Entity)> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.scope_checks_pending(),
            ServerImpl::Full(server) => server.scope_checks_pending(),
        }
    }

    pub fn mark_scope_checks_pending_handled(&mut self) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.mark_scope_checks_pending_handled(),
            ServerImpl::Full(server) => server.mark_scope_checks_pending_handled(),
        }
    }

    pub fn mark_all_scope_checks_pending(&mut self) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.mark_all_scope_checks_pending(),
            ServerImpl::Full(server) => server.mark_all_scope_checks_pending(),
        }
    }

    //// Users ////

    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_exists(user_key),
            ServerImpl::Full(server) => server.user_exists(user_key),
        }
    }

    pub fn user(&'_ self, user_key: &UserKey) -> UserRef<'_, Entity> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user(user_key),
            ServerImpl::Full(server) => server.user(user_key),
        }
    }

    pub fn user_opt(&'_ self, user_key: &UserKey) -> Option<UserRef<'_, Entity>> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_opt(user_key),
            ServerImpl::Full(server) => server.user_opt(user_key),
        }
    }

    pub fn user_mut(&'_ mut self, user_key: &UserKey) -> UserMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_mut(user_key),
            ServerImpl::Full(server) => server.user_mut(user_key),
        }
    }

    pub fn user_mut_opt(&'_ mut self, user_key: &UserKey) -> Option<UserMut<'_, Entity>> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_mut_opt(user_key),
            ServerImpl::Full(server) => server.user_mut_opt(user_key),
        }
    }

    pub fn user_keys(&self) -> Vec<UserKey> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_keys(),
            ServerImpl::Full(server) => server.user_keys(),
        }
    }

    pub fn users_count(&self) -> usize {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.users_count(),
            ServerImpl::Full(server) => server.users_count(),
        }
    }

    pub fn user_count(&self) -> usize {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_count(),
            ServerImpl::Full(server) => server.user_count(),
        }
    }

    pub fn entity_count(&self) -> usize {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.entity_count(),
            ServerImpl::Full(server) => server.entity_count(),
        }
    }

    pub fn user_scope(&'_ self, user_key: &UserKey) -> UserScopeRef<'_, Entity> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_scope(user_key),
            ServerImpl::Full(server) => server.user_scope(user_key),
        }
    }

    pub fn user_scope_mut(&'_ mut self, user_key: &UserKey) -> UserScopeMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_scope_mut(user_key),
            ServerImpl::Full(server) => server.user_scope_mut(user_key),
        }
    }

    //// Priority ////

    pub fn global_entity_priority(&self, entity: Entity) -> EntityPriorityRef<'_, Entity> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.global_entity_priority(entity),
            ServerImpl::Full(server) => server.global_entity_priority(entity),
        }
    }

    pub fn global_entity_priority_mut(&mut self, entity: Entity) -> EntityPriorityMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.global_entity_priority_mut(entity),
            ServerImpl::Full(server) => server.global_entity_priority_mut(entity),
        }
    }

    pub fn user_entity_priority(
        &self,
        user_key: &UserKey,
        entity: Entity,
    ) -> EntityPriorityRef<'_, Entity> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_entity_priority(user_key, entity),
            ServerImpl::Full(server) => server.user_entity_priority(user_key, entity),
        }
    }

    pub fn user_entity_priority_mut(
        &mut self,
        user_key: &UserKey,
        entity: Entity,
    ) -> EntityPriorityMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_entity_priority_mut(user_key, entity),
            ServerImpl::Full(server) => server.user_entity_priority_mut(user_key, entity),
        }
    }

    //// Rooms ////

    pub fn create_room(&'_ mut self) -> RoomMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.create_room(),
            ServerImpl::Full(server) => server.create_room(),
        }
    }

    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.room_exists(room_key),
            ServerImpl::Full(server) => server.room_exists(room_key),
        }
    }

    pub fn room(&'_ self, room_key: &RoomKey) -> RoomRef<'_, Entity> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.room(room_key),
            ServerImpl::Full(server) => server.room(room_key),
        }
    }

    pub fn room_mut(&'_ mut self, room_key: &RoomKey) -> RoomMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.room_mut(room_key),
            ServerImpl::Full(server) => server.room_mut(room_key),
        }
    }

    pub fn room_keys(&self) -> Vec<RoomKey> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.room_keys(),
            ServerImpl::Full(server) => server.room_keys(),
        }
    }

    pub fn rooms_count(&self) -> usize {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.rooms_count(),
            ServerImpl::Full(server) => server.rooms_count(),
        }
    }

    pub fn room_count(&self) -> usize {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.room_count(),
            ServerImpl::Full(server) => server.room_count(),
        }
    }

    //// Ticks ////

    pub fn current_tick(&self) -> Tick {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.current_tick(),
            ServerImpl::Full(server) => server.current_tick(),
        }
    }

    pub fn average_tick_duration(&self) -> Duration {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.average_tick_duration(),
            ServerImpl::Full(server) => server.average_tick_duration(),
        }
    }

    //// Network Conditions ////

    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.jitter(user_key),
            ServerImpl::Full(server) => server.jitter(user_key),
        }
    }

    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.rtt(user_key),
            ServerImpl::Full(server) => server.rtt(user_key),
        }
    }

    pub fn connection_stats(&self, user_key: &UserKey) -> Option<ConnectionStats> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.connection_stats(user_key),
            ServerImpl::Full(server) => server.connection_stats(user_key),
        }
    }

    pub fn enable_historian(&mut self, max_ticks: u16) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.enable_historian(max_ticks),
            ServerImpl::Full(server) => server.enable_historian(max_ticks),
        }
    }

    pub fn record_historian_tick<W: WorldRefType<Entity>>(&mut self, world: W, tick: Tick) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.record_historian_tick(world, tick),
            ServerImpl::Full(server) => server.record_historian_tick(world, tick),
        }
    }

    pub fn historian(&self) -> Option<&Historian> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.historian(),
            ServerImpl::Full(server) => server.historian(),
        }
    }

    // Entity Replication

    pub(crate) fn enable_replication(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.enable_entity_replication(entity),
            ServerImpl::Full(server) => server.enable_entity_replication(entity),
        }
    }

    pub(crate) fn disable_replication(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.disable_entity_replication(entity),
            ServerImpl::Full(server) => server.disable_entity_replication(entity),
        }
    }

    pub(crate) fn pause_replication(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.pause_entity_replication(entity),
            ServerImpl::Full(server) => server.pause_entity_replication(entity),
        }
    }

    pub(crate) fn resume_replication(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.resume_entity_replication(entity),
            ServerImpl::Full(server) => server.resume_entity_replication(entity),
        }
    }

    pub(crate) fn replication_config(&self, entity: &Entity) -> Option<ReplicationConfig> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.entity_replication_config(entity),
            ServerImpl::Full(server) => server.entity_replication_config(entity),
        }
    }

    pub fn entity_take_authority(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => {
                let _ = server.entity_take_authority(entity);
            }
            ServerImpl::Full(server) => {
                let _ = server.entity_take_authority(entity);
            }
        }
    }

    pub(crate) fn entity_give_authority(&mut self, entity: &Entity, user_key: &UserKey) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => {
                let _ = server.entity_give_authority(user_key, entity);
            }
            ServerImpl::Full(server) => {
                let _ = server.entity_give_authority(user_key, entity);
            }
        }
    }

    pub(crate) fn entity_authority_status(&self, _entity: &Entity) -> Option<EntityAuthStatus> {
        todo!("entity_authority_status requires world access; use ServerImpl directly in exclusive systems")
    }

    // ====================================================================
    // Replicated Resources — read-only / status surface on the
    // Server SystemParam.
    //
    // World-mutating ops (insert/remove/configure) live on the
    // `CommandsExt` trait (see `commands.rs`) and follow the standard
    // Bevy `Command` queue pattern — `commands.replicate_resource(v)`
    // queues a `ReplicateResourceCommand<R>` that runs with `&mut World`
    // and dispatches into `ServerImpl` via `world.resource_scope`.
    // ====================================================================

    /// True iff a Replicated Resource of type `R` is currently inserted
    /// on this server.
    pub fn has_resource<R: ReplicatedResource>(&self) -> bool {
        self.server_impl.has_resource::<R>()
    }

    /// Returns the hidden Bevy `Entity` carrying resource `R`, or
    /// `None` if `R` is not currently inserted. Mostly useful for
    /// advanced introspection / diagnostics; user code should typically
    /// access resource state through `Res<R>` — under bevy 0.19 the
    /// carrier entity-component aliases `Res<R>` (one storage cell), so
    /// no mirror system is involved.
    pub fn resource_entity<R: ReplicatedResource>(&self) -> Option<Entity> {
        self.server_impl.resource_entity::<R>()
    }

    /// True iff `entity` is the hidden entity for any Replicated
    /// Resource. Used by the event-emission filter (D13) to suppress
    /// SpawnEntityEvent / component events for resource entities.
    pub fn is_resource_entity(&self, entity: &Entity) -> bool {
        self.server_impl.is_resource_entity(entity)
    }

    /// Number of currently-inserted resources.
    pub fn resources_count(&self) -> usize {
        self.server_impl.resources_count()
    }

    /// Server-side authority status for resource `R`. `None` if not
    /// inserted or not delegable.
    pub fn resource_authority_status<R: ReplicatedResource>(&self) -> Option<EntityAuthStatus> {
        self.server_impl.resource_authority_status::<R>()
    }

    pub fn world_only_resource_scope<R>(
        world: &mut World,
        f: impl FnOnce(&mut World, &mut NaiaWorldServer<Entity>) -> R,
    ) -> R {
        world.resource_scope(|world, mut server: Mut<ServerImpl>| match &mut *server {
            ServerImpl::WorldOnly(server) => f(world, server),
            ServerImpl::Full(_) => {
                panic!("Expected WorldOnly Server, found Full Server");
            }
        })
    }

    /// Pipeline-mode coordinator helper (step 4-F.cyberlith.d): apply a
    /// `ReceiveOutput<Entity>` returned by `WorldServer::receive()` to the
    /// Bevy `World`, firing the same Bevy events that the in-Update
    /// `translate_world_events` / `translate_tick_events` systems fire in
    /// the non-pipeline mode.
    ///
    /// Internally pulls the `ServerImpl` resource out of `World` via
    /// `resource_scope` (so the caller doesn't need to reach for the
    /// private wrapper), then delegates to
    /// [`crate::apply_receive_output::apply_receive_output`].
    pub fn apply_receive_output(world: &mut World, output: naia_server::ReceiveOutput<Entity>) {
        world.resource_scope(|world, mut server: Mut<ServerImpl>| {
            crate::apply_receive_output::apply_receive_output(world, &mut *server, output);
        });
    }

    // ====================================================================
    // Pipelined-server lifecycle (§2f) — static helpers that drive the
    // pipeline stored in the `ServerImpl::WorldOnly(WorldServer)` resource.
    //
    // The pipelined plugin (`Plugin::pipelined`) installs a `WorldServer`
    // built via `WorldServer::from_pipelined`, so the underlying
    // `PipelinedWorldServer` is reachable through the unified enum. These
    // replace the removed `PluginInternalState` lifecycle surface.
    // ====================================================================

    /// Build the per-stage timing hooks for the pipeline runtime. Empty
    /// (zero-overhead) unless the `pipeline_timing` bench feature is on.
    fn pipeline_timing_hooks() -> naia_server::pipeline_actors::RuntimeTimingHooks {
        #[allow(unused_mut)]
        let mut timing = naia_server::pipeline_actors::RuntimeTimingHooks::default();
        #[cfg(feature = "pipeline_timing")]
        {
            timing.record_recv = Some(crate::pipeline_timing::record_recv);
            timing.record_send = Some(crate::pipeline_timing::record_send);
            timing.record_barrier = Some(crate::pipeline_timing::record_barrier);
        }
        timing
    }

    /// Bind the pipeline's socket (the bind-only startup step). Pair with
    /// [`Self::pipeline_start`], which spawns the worker runtime.
    pub fn pipeline_listen(world: &mut World, socket: impl Into<Box<dyn Socket>>) {
        Self::world_only_resource_scope(world, |_world, ws| ws.listen(socket));
    }

    /// The separate spawn step: stand up the worker runtime (`Armed →
    /// Running`). Call after [`Self::pipeline_listen`]. No-op in
    /// deterministic builds (no worker threads).
    pub fn pipeline_start(world: &mut World) {
        let timing = Self::pipeline_timing_hooks();
        Self::world_only_resource_scope(world, |_world, ws| ws.start_workers(timing));
    }

    /// Park both worker threads (open the park window). No-op when not
    /// `Running`. Safe because the pipelined plugin only ever installs a
    /// Pipelined `WorldServer`.
    pub fn pipeline_park(world: &World) {
        if let Some(ws) = world.resource::<ServerImpl>().as_world_server() {
            ws.park_workers();
        }
    }

    /// Park both worker threads, first **flushing the send pipeline to io** so
    /// every snapshot produced up to this point is transmitted before the
    /// consumer advances its clock. The deterministic delivery barrier a
    /// test/harness bounded-tick liveness poll needs under active workers; a
    /// plain park otherwise. No-op when not `Running`.
    pub fn pipeline_park_flushing(world: &World) {
        if let Some(ws) = world.resource::<ServerImpl>().as_world_server() {
            ws.park_workers_flushing();
        }
    }

    /// Resume both worker threads (close the park window). No-op when not
    /// `Running`.
    pub fn pipeline_unpark(world: &World) {
        if let Some(ws) = world.resource::<ServerImpl>().as_world_server() {
            ws.unpark_workers();
        }
    }

    /// G1 — park + drain (the recv half of the park-window bracket). Owns the
    /// resource-scope nesting, the coord-handle acquisition, and the per-output
    /// loop, so the consumer supplies only the recv-world choice and the
    /// per-output applier (policy). Replaces the hand-rolled
    /// `world_only_resource_scope` + `as_pipelined().expect().coord()` + output
    /// loop in a consumer's `open_park_window`.
    ///
    /// `recv_world`: `None` receives entity ops into `world` itself (the
    /// single-world path); `Some(sim)` receives into a disjoint sim world (the
    /// delegated/editor path). For each non-empty output the applier is called
    /// with `(&mut world, recv_world, coord, output)`.
    pub fn pipeline_recv(
        world: &mut World,
        mut recv_world: Option<&mut World>,
        mut apply: impl FnMut(
            &mut World,
            Option<&mut World>,
            &naia_server::WorldServer<Entity>,
            naia_server::ReceiveOutput<Entity>,
        ),
    ) {
        Self::world_only_resource_scope(world, |world, ws| {
            let outputs = match recv_world.as_deref_mut() {
                Some(sim) => ws.receive(naia_bevy_shared::WorldProxyMut::proxy_mut(&mut *sim)),
                None => ws.receive(naia_bevy_shared::WorldProxyMut::proxy_mut(&mut *world)),
            };
            for output in outputs {
                if output.is_empty() {
                    continue;
                }
                apply(world, recv_world.as_deref_mut(), ws, output);
            }
        });
    }

    /// G1 — snapshot + unpark (the send half of the park-window bracket). Owns
    /// the resource-scope + immutable world proxy. Replaces the consumer's
    /// `world_only_resource_scope(|_, ws| ws.send(WorldProxy::proxy(..)))` in
    /// `close_park_window`.
    pub fn pipeline_send(world: &mut World, send_world: &World) {
        Self::world_only_resource_scope(world, |_world, ws| {
            ws.send(naia_bevy_shared::WorldProxy::proxy(send_world));
        });
    }

    /// G6 — register a replicated resource from a sim-mode (pipelined) world.
    /// The sim-mode counterpart of the resident
    /// [`crate::ServerCommandsExt::replicate_resource`].
    ///
    /// Under bevy's resource/storage aliasing the inserted value's component
    /// lives on a hidden backing carrier entity; this tags that carrier
    /// `HostOwned` (so it rides the existing `HostSyncEvent` machinery exactly
    /// like an avatar/tile) and enqueues it for the coord-side registration drain
    /// ([`Self::pipeline_drain_resource_registrations`]). The value insert + the
    /// `HostOwned` attach are the same `&mut World` op, so naia's
    /// `on_component_added::<R>` fires naturally on the next sim Update.
    pub fn pipeline_replicate_resource<R: ReplicatedResource>(sim_world: &mut World, value: R) {
        sim_world.insert_resource(value);
        let Some(entity) = Self::resource_backing_entity::<R>(sim_world) else {
            return;
        };
        // Carrier reuse across remove/re-insert: a carrier that ALREADY has
        // `HostOwned` is a prior carrier being re-used — its naia registration is
        // still live; only the component was re-added. Enable once per entity —
        // re-enabling fails loud in naia, and the re-add's `on_component_added`
        // carries the existing GlobalEntity through the host-sync drain.
        if sim_world.entity(entity).contains::<HostOwned>() {
            return;
        }
        sim_world
            .entity_mut(entity)
            .insert(HostOwned::new::<Singleton>());
        sim_world
            .get_resource_or_insert_with(PendingResourceRegistrations::default)
            .0
            .push(entity);
    }

    /// G6 — de-register a sim-mode replicated resource. Removes ONLY the resource
    /// component and deliberately KEEPS `HostOwned` on the carrier: naia's
    /// `on_component_removed::<R>` emits the wire-Remove only while the carrier
    /// still has `HostOwned`, so dropping it here would SUPPRESS the removal and
    /// clients' `Res<R>` would never clear. With `HostOwned` retained, the
    /// component-remove replicates → clients clear the carrier component → their
    /// `Res<R>` becomes `None` via storage aliasing.
    ///
    /// The now-empty carrier entity is intentionally retained (exactly as bevy's
    /// own `remove_resource` retains its backing entity) and is reused by the next
    /// [`Self::pipeline_replicate_resource`] — a former-resource entity can't be
    /// despawned under bevy 0.19.
    pub fn pipeline_remove_replicated_resource<R: ReplicatedResource>(sim_world: &mut World) {
        sim_world.remove_resource::<R>();
    }

    /// Locate the hidden backing carrier entity for resource `R` (the `Res<R>`
    /// component lives on it via storage aliasing).
    fn resource_backing_entity<R: Component>(world: &mut World) -> Option<Entity> {
        let mut query = world.query_filtered::<Entity, bevy_ecs::query::With<R>>();
        query.iter(world).next()
    }

    /// G6 — coord-side drain of [`PendingResourceRegistrations`]; call inside the
    /// park window. For each pending carrier: enable replication, apply the
    /// caller's `config` (policy), add it to `room_key` (policy), then flush the
    /// batched world hooks. Byte-identical to the former hand-rolled
    /// `enable → configure → room_add → apply_pending_world_hooks` sequence; the
    /// deferred Send-side `ConfigureReplication` ops queued here drain later inside
    /// `send()`'s D8 preamble, as before.
    pub fn pipeline_drain_resource_registrations(
        main_world: &mut World,
        sim_world: &mut World,
        room_key: &RoomKey,
        config: ReplicationConfig,
    ) {
        let pending: Vec<Entity> = sim_world
            .get_resource_mut::<PendingResourceRegistrations>()
            .map(|mut r| std::mem::take(&mut r.0))
            .unwrap_or_default();
        if pending.is_empty() {
            return;
        }
        Self::world_only_resource_scope(main_world, |_main, ws| {
            let mut proxy = WorldProxyMut::proxy_mut(&mut *sim_world);
            for entity in &pending {
                ws.enable_entity_replication(entity);
                ws.configure_entity_replication(&mut proxy, entity, config);
                ws.room_add_entity(room_key, entity);
            }
            ws.apply_pending_world_hooks(&mut proxy);
        });
    }

    /// G6 (single-world) — coordinator + entities share ONE `World`
    /// (MISSION_SINGLE_WORLD_CELL). Byte-identical to the two-world
    /// [`Self::pipeline_drain_resource_registrations`]: same
    /// `enable → configure → room_add → apply_pending_world_hooks` sequence in the
    /// same order, with the same deferred Send-side `ConfigureReplication`
    /// queuing. The only difference is the source of the entity proxy: the
    /// `PendingResourceRegistrations` queue is drained from `world` BEFORE the
    /// `ServerImpl` resource is lifted out, then the de-resourced `world` handed
    /// back by [`Self::world_only_resource_scope`] is used as the proxy source —
    /// so the coord borrow and the entity borrow never alias (which is exactly
    /// why the two-`&mut World` signature can't serve a single world). Call inside
    /// the park window. No-op early-return when the outbox is empty.
    pub fn pipeline_drain_resource_registrations_single(
        world: &mut World,
        room_key: &RoomKey,
        config: ReplicationConfig,
    ) {
        let pending: Vec<Entity> = world
            .get_resource_mut::<PendingResourceRegistrations>()
            .map(|mut r| std::mem::take(&mut r.0))
            .unwrap_or_default();
        if pending.is_empty() {
            return;
        }
        Self::world_only_resource_scope(world, |world, ws| {
            let mut proxy = WorldProxyMut::proxy_mut(&mut *world);
            for entity in &pending {
                ws.enable_entity_replication(entity);
                ws.configure_entity_replication(&mut proxy, entity, config);
                ws.room_add_entity(room_key, entity);
            }
            ws.apply_pending_world_hooks(&mut proxy);
        });
    }

    /// G6b — drain the sim world's `HostSyncEvent` queue against the parked
    /// pipeline (the D5 host-sync phase); call inside the park window. Relocates
    /// the former consumer-side `take_handles → drain → restore_handles` dance
    /// into the engine: the handle take/restore is now naia's own internal
    /// concern (like `receive`/`send`), so consumers no longer reach for the raw
    /// pipeline handles.
    ///
    /// The drain genuinely needs the handles taken out (it calls `&mut SendHandle`
    /// worldless insert/remove/despawn ops + `&mut CoordHandle::state`, which are
    /// not reachable through the in-place `as_pipelined_mut` forwarding methods).
    /// The take/restore is wrapped so the handles are restored to their slots even
    /// if the drain unwinds — otherwise a mid-drain panic would leave the slots
    /// empty and mask the real fault with a misleading "park workers first" on the
    /// next tick. The panic is re-raised after restore.
    pub fn pipeline_drain_host_sync(main_world: &mut World, sim_world: &mut World) {
        Self::world_only_resource_scope(main_world, |_main, ws| {
            let (mut coord, recv, mut send) = ws.take_handles();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::host_sync_pipeline::drain_host_sync_in_place(
                    sim_world, &mut coord, &mut send,
                );
            }));
            // Restore unconditionally — the handles are still owned here on both
            // the success and unwind paths (the drain borrowed them).
            ws.restore_handles(coord, recv, send);
            if let Err(payload) = result {
                std::panic::resume_unwind(payload);
            }
        });
    }

    /// G6b (single-world) — coordinator + entities share ONE `World`
    /// (MISSION_SINGLE_WORLD_CELL). Byte-identical to the two-world
    /// [`Self::pipeline_drain_host_sync`]: lift the `ServerImpl` out, take the
    /// handles, drain `Messages<HostSyncEvent>` via the same in-place core, and
    /// restore — wrapped in the same `catch_unwind` so a mid-drain panic still
    /// restores the handles to their slots (then re-raises) rather than masking
    /// the fault with a misleading "park workers first" on the next tick. The
    /// only difference is the entity source: the de-resourced `world` from
    /// [`Self::world_only_resource_scope`] is the drain target, so the coord
    /// borrow and the entity borrow never alias. Call inside the park window.
    pub fn pipeline_drain_host_sync_single(world: &mut World) {
        Self::world_only_resource_scope(world, |world, ws| {
            let (mut coord, recv, mut send) = ws.take_handles();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::host_sync_pipeline::drain_host_sync_in_place(world, &mut coord, &mut send);
            }));
            ws.restore_handles(coord, recv, send);
            if let Err(payload) = result {
                std::panic::resume_unwind(payload);
            }
        });
    }

    /// G5b — coord-side drain of [`PendingScopeAuthorityOps`]; call inside the
    /// park window, BEFORE the host-sync drain (a `TakeAuthority` must land before
    /// its reject/replace despawn replicates). Relocates the former consumer-side
    /// `with_world_server` reassembly + raw `room_add_entity` / `entity_take_authority`
    /// calls into the engine.
    ///
    /// Order matches the prior hand-rolled path: all `RoomAdd`s first (in queue
    /// order, via the Coord-only `as_pipelined_mut().room_add_entity`), then the
    /// authority ops (`TakeAuthority` / `GiveAuthority`) in queue order — so a
    /// room-add is normalized ahead of any authority op. Byte-identical to the
    /// legacy two-phase drain. No-op early-return when the queue is empty.
    pub fn pipeline_drain_scope_authority_ops(
        main_world: &mut World,
        sim_world: &mut World,
        room_key: &RoomKey,
    ) {
        let ops: Vec<PipelineScopeAuthorityOp> = sim_world
            .get_resource_mut::<PendingScopeAuthorityOps>()
            .map(|mut q| std::mem::take(&mut q.0))
            .unwrap_or_default();
        if ops.is_empty() {
            return;
        }
        Self::world_only_resource_scope(main_world, |_main, ws| {
            // Phase 1: room adds (first-class unified op).
            for op in &ops {
                if let PipelineScopeAuthorityOp::RoomAdd(entity) = op {
                    ws.room_add_entity(room_key, entity);
                }
            }
            // Phase 2: authority ops, in queue order.
            for op in &ops {
                match op {
                    PipelineScopeAuthorityOp::TakeAuthority(entity) => {
                        let _ = ws.entity_take_authority(entity);
                    }
                    PipelineScopeAuthorityOp::GiveAuthority(user, entity) => {
                        let _ = ws.entity_give_authority(user, entity);
                    }
                    PipelineScopeAuthorityOp::RoomAdd(_) => {}
                }
            }
        });
    }

    /// Re-panic on the main thread if a worker thread has panicked.
    pub fn pipeline_propagate_panics(world: &World) {
        if let Some(ws) = world.resource::<ServerImpl>().as_world_server() {
            ws.propagate_panic_if_any();
        }
    }

    /// `true` once the worker runtime is `Running`. `false` for a resident
    /// (`Full`) server or a not-yet-started / deterministic pipeline.
    pub fn pipeline_is_running(world: &World) -> bool {
        world
            .resource::<ServerImpl>()
            .as_world_server()
            .map_or(false, |ws| ws.is_running())
    }

    /// Test/dev hook: request that the pipeline workers panic on their next
    /// loop iteration. No-op when no worker runtime is present.
    #[cfg(any(test, feature = "test_time"))]
    pub fn pipeline_request_worker_panic_for_test(world: &World) {
        if let Some(ws) = world.resource::<ServerImpl>().as_world_server() {
            if ws.mode() == naia_server::ServerMode::Pipelined {
                ws.request_worker_panic_for_test();
            }
        }
    }
}

impl<'w> EntityAndGlobalEntityConverter<Entity> for Server<'w> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<Entity, EntityDoesNotExistError> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.global_entity_to_entity(global_entity),
            ServerImpl::Full(server) => server.global_entity_to_entity(global_entity),
        }
    }

    fn entity_to_global_entity(
        &self,
        entity: &Entity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.entity_to_global_entity(entity),
            ServerImpl::Full(server) => server.entity_to_global_entity(entity),
        }
    }
}
