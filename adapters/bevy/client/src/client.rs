use std::{marker::PhantomData, net::SocketAddr, time::Duration};

use bevy_ecs::{
    entity::Entity,
    resource::Resource,
    system::{ResMut, SystemParam},
    world::{Mut, World},
};

use naia_bevy_shared::{
    Channel, EntityAndGlobalEntityConverter, EntityAuthStatus, EntityDoesNotExistError,
    GlobalEntity, Message, Request, Response, ResponseReceiveKey, ResponseSendKey, Tick,
    WorldProxyMut,
};
use naia_client::{
    shared::{GameInstant, ReplicatedComponent, SocketConfig},
    transport::Socket,
    Client as NaiaClient, ConnectionStats, ConnectionStatus, EntityPriorityMut, EntityPriorityRef,
    NaiaClientError,
};

use crate::Publicity;

/// Bevy resource wrapping the underlying naia `Client`, scoped to client-tag
/// `T` so a multi-client app can hold one per connection.
#[derive(Resource)]
pub struct ClientWrapper<T: Send + Sync + 'static> {
    /// The wrapped naia client, keyed on Bevy's `Entity` type.
    pub client: NaiaClient<Entity>,
    phantom_t: PhantomData<T>,
}

impl<T: Send + Sync + 'static> ClientWrapper<T> {
    /// Wraps `client` for storage as a Bevy resource under client-tag `T`.
    pub fn new(client: NaiaClient<Entity>) -> Self {
        Self {
            client,
            phantom_t: PhantomData,
        }
    }
}

/// Bevy `SystemParam` wrapping the client-tag-`T` naia client, giving
/// systems access to connection control, messaging, ticks, and entity
/// priority/authority.
#[derive(SystemParam)]
pub struct Client<'w, T: Send + Sync + 'static> {
    client: ResMut<'w, ClientWrapper<T>>,
}

impl<'w, T: Send + Sync + 'static> Client<'w, T> {
    // Public Methods //

    //// Connections ////

    /// Sets the authentication payload sent with the pending connection
    /// request.
    pub fn auth<M: Message>(&mut self, auth: M) {
        self.client.client.auth(auth);
    }

    /// Sets the HTTP headers sent with the connection handshake (e.g. for
    /// WebSocket/WebRTC signaling that needs custom headers).
    pub fn auth_headers(&mut self, headers: Vec<(String, String)>) {
        self.client.client.auth_headers(headers);
    }

    /// Starts connecting to the server over `socket`.
    pub fn connect<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        self.client.client.connect(socket);
    }

    /// Disconnects from the server.
    pub fn disconnect(&mut self) {
        self.client.client.disconnect();
    }

    /// Returns the current connection status (connecting, connected, or
    /// disconnected).
    pub fn connection_status(&self) -> ConnectionStatus {
        self.client.client.connection_status()
    }

    /// Returns the server's socket address once connected, or an error if
    /// it is not yet known.
    pub fn server_address(&self) -> Result<SocketAddr, NaiaClientError> {
        self.client.client.server_address()
    }

    /// Returns the current measured round-trip time, in milliseconds.
    pub fn rtt(&self) -> f32 {
        self.client.client.rtt()
    }

    /// Returns the current measured jitter, in milliseconds.
    pub fn jitter(&self) -> f32 {
        self.client.client.jitter()
    }

    // Config
    /// Returns the socket configuration this client was created with.
    pub fn socket_config(&self) -> &SocketConfig {
        self.client.client.socket_config()
    }

    //// Messages ////
    /// Sends `message` on channel `C`. Fails if the channel's direction
    /// does not allow client-to-server sends.
    pub fn send_message<C: Channel, M: Message>(
        &mut self,
        message: &M,
    ) -> Result<(), NaiaClientError> {
        self.client.client.send_message::<C, M>(message)
    }

    /// Sends `message` on tick-buffered channel `C`, tagged with the given
    /// `tick` for server-side tick-buffer replay.
    pub fn send_tick_buffer_message<C: Channel, M: Message>(&mut self, tick: &Tick, message: &M) {
        self.client
            .client
            .send_tick_buffer_message::<C, M>(tick, message);
    }

    //// Requests ////
    /// Sends `request` on channel `C` and returns a key for later receiving
    /// the server's typed response via [`receive_response`](Self::receive_response).
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaClientError> {
        self.client.client.send_request::<C, Q>(request)
    }

    /// Sends `response` to the request identified by `response_key`.
    /// Returns `false` if the key is unknown or already answered.
    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        self.client.client.send_response(response_key, response)
    }

    /// Returns the server's response for `response_key` if it has arrived,
    /// consuming the pending receipt.
    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<S> {
        self.client.client.receive_response(response_key)
    }

    //// Ticks ////

    /// Returns the client's current tick, or `None` before the first tick
    /// is known.
    pub fn client_tick(&self) -> Option<Tick> {
        self.client.client.client_tick()
    }

    /// Returns the `GameInstant` corresponding to the client's current tick.
    pub fn client_instant(&self) -> Option<GameInstant> {
        self.client.client.client_instant()
    }

    /// Returns the most recently received server tick, or `None` before any
    /// server tick is known.
    pub fn server_tick(&self) -> Option<Tick> {
        self.client.client.server_tick()
    }

    /// Returns the `GameInstant` corresponding to the most recently
    /// received server tick.
    pub fn server_instant(&self) -> Option<GameInstant> {
        self.client.client.server_instant()
    }

    /// Converts `tick` to its corresponding `GameInstant`, or `None` if the
    /// conversion is not yet possible.
    pub fn tick_to_instant(&self, tick: Tick) -> Option<GameInstant> {
        self.client.client.tick_to_instant(tick)
    }

    /// Returns the configured tick duration, or `None` before it is known.
    pub fn tick_duration(&self) -> Option<Duration> {
        self.client.client.tick_duration()
    }

    // Interpolation

    /// Returns the current client-side interpolation factor (0.0–1.0
    /// between the previous and current client tick), or `None` if
    /// unavailable.
    pub fn client_interpolation(&self) -> Option<f32> {
        self.client.client.client_interpolation()
    }

    /// Returns the current server-side interpolation factor (0.0–1.0
    /// between the previous and current server tick), or `None` if
    /// unavailable.
    pub fn server_interpolation(&self) -> Option<f32> {
        self.client.client.server_interpolation()
    }

    // Entity Registration

    pub(crate) fn enable_replication(&mut self, entity: &Entity) {
        self.client.client.enable_entity_replication(entity);
    }

    pub(crate) fn disable_replication(&mut self, entity: &Entity) {
        self.client.client.disable_entity_replication(entity);
    }

    pub(crate) fn replication_config(&self, entity: &Entity) -> Option<Publicity> {
        self.client.client.entity_replication_config(entity)
    }

    pub(crate) fn entity_request_authority(&mut self, entity: &Entity) {
        let _ = self.client.client.entity_request_authority(entity);
    }

    pub(crate) fn entity_release_authority(&mut self, entity: &Entity) {
        let _ = self.client.client.entity_release_authority(entity);
    }

    pub(crate) fn entity_authority_status(&self, entity: &Entity) -> Option<EntityAuthStatus> {
        self.client.client.entity_authority_status(entity)
    }

    /// Queues a despawn notification to the server for `entity` and removes
    /// the entity record from naia's global tracking (`entity_records`).
    ///
    /// Normally callers should just call `commands.entity(entity).despawn()`
    /// and let the Bevy adapter's `on_despawn` system route the notification
    /// during the next tick. Use this method directly only when you need to
    /// emit the despawn notification *before* the Bevy entity is removed —
    /// for example, to bias the host-channel ordering against an in-flight
    /// server update for the same entity in tick-locked test harnesses.
    ///
    /// The notification itself is still flushed by the next `send_packets`
    /// pass, so this is an ordering primitive, not a synchronous despawn.
    /// Calling it again from `on_despawn` is harmless: the second call
    /// returns early because `entity_records` no longer contains the entity.
    pub fn despawn_entity_worldless(&mut self, entity: &Entity) {
        self.client.client.despawn_entity_worldless(entity);
    }

    //// Priority ////

    /// Returns a read-only handle to `entity`'s replication priority.
    pub fn entity_priority(&self, entity: Entity) -> EntityPriorityRef<'_, Entity> {
        self.client.client.entity_priority(entity)
    }

    /// Returns a mutable handle to `entity`'s replication priority.
    pub fn entity_priority_mut(&mut self, entity: Entity) -> EntityPriorityMut<'_, Entity> {
        self.client.client.entity_priority_mut(entity)
    }

    /// Returns current connection statistics, or `None` if not connected.
    pub fn connection_stats(&self) -> Option<ConnectionStats> {
        self.client.client.connection_stats()
    }
}

impl<'w, T: Send + Sync + 'static> EntityAndGlobalEntityConverter<Entity> for Client<'w, T> {
    fn global_entity_to_entity(
        &self,
        global_entity: GlobalEntity,
    ) -> Result<Entity, EntityDoesNotExistError> {
        self.client.client.global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        entity: &Entity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.client.client.entity_to_global_entity(entity)
    }
}

/// Drains component `R`'s freshly-decoded-but-not-yet-applied updates from the receive
/// buffer, applies each to the world in tick order, and returns the resulting value per
/// update as `(Tick, Entity, R)`. Drained entries are not redone by the later full apply
/// (`ProcessPackets`); all other buffered updates and inserts are left for that apply.
///
/// Intended for an exclusive system running between decode (`HandleTickEvents`) and the
/// full apply: it exposes a remote *input* component's PER-TICK history (e.g. a remote
/// avatar's command at each tick it changed) before a tick, so a deterministic
/// re-simulation can re-derive each catch-up tick with that tick's own input, while the
/// remaining *state* reconciles afterward. `T` is the protocol marker. See
/// `LocalWorldManager::take_received_updates_of_kind`.
pub fn take_received_updates_of_kind<T: Send + Sync + 'static, R: ReplicatedComponent>(
    world: &mut World,
) -> Vec<(Tick, Entity, R)> {
    let mut result = Vec::new();
    world.resource_scope(|world, mut client: Mut<ClientWrapper<T>>| {
        result = client
            .client
            .take_received_updates_of_kind::<R, _>(world.proxy_mut());
    });
    result
}
