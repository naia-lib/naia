use std::{
    any::Any, collections::HashMap, hash::Hash, marker::PhantomData, mem, net::SocketAddr,
    vec::IntoIter,
};

use log::warn;

use naia_shared::{
    Channel, ChannelKind, ComponentKind, DisconnectReason, GlobalResponseId, Message,
    MessageContainer, MessageKind, Replicate, Request, ResponseSendKey,
};

use crate::{user::UserKey, ConnectEvent, ErrorEvent, NaiaServerError};

pub(crate) type MessagesMap = HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>>;
pub(crate) type RequestsMap = HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, GlobalResponseId, MessageContainer)>>>;
pub(crate) type RemovesMap<E> = HashMap<ComponentKind, Vec<(UserKey, E, Box<dyn Replicate>)>>;
/// Kind-only remove events — fired when entity component data is not available
/// (e.g. client-owned entity despawn, where the server does not hold component bytes).
pub(crate) type RemovesSyntheticMap<E> = HashMap<ComponentKind, Vec<(UserKey, E)>>;

/// Per-tick event container carrying all entity-lifecycle, message, and component events.
///
/// Returned by [`Server::take_world_events`](crate::Server::take_world_events).
/// Prefer iterating via `read::<SomeEvent>()`.
pub struct WorldEvents<E: Hash + Copy + Eq + Sync + Send> {
    connections: Vec<UserKey>,
    disconnections: Vec<(UserKey, SocketAddr, DisconnectReason)>,
    errors: Vec<NaiaServerError>,
    messages: MessagesMap,
    requests: RequestsMap,
    spawns: Vec<(UserKey, E)>,
    despawns: Vec<(UserKey, E)>,
    publishes: Vec<(UserKey, E)>,
    unpublishes: Vec<(UserKey, E)>,
    delegates: Vec<(UserKey, E)>,
    auth_grants: Vec<(UserKey, E)>,
    auth_denials: Vec<(UserKey, E)>,
    auth_resets: Vec<E>,
    inserts: HashMap<ComponentKind, Vec<(UserKey, E)>>,
    removes: RemovesMap<E>,
    removes_synthetic: RemovesSyntheticMap<E>,
    updates: HashMap<ComponentKind, Vec<(UserKey, E)>>,
    empty: bool,
}

impl<E: Hash + Copy + Eq + Sync + Send> WorldEvents<E> {
    pub(crate) fn new() -> Self {
        Self {
            connections: Vec::new(),
            disconnections: Vec::new(),
            errors: Vec::new(),
            messages: HashMap::new(),
            requests: HashMap::new(),
            spawns: Vec::new(),
            despawns: Vec::new(),
            publishes: Vec::new(),
            unpublishes: Vec::new(),
            delegates: Vec::new(),
            auth_grants: Vec::new(),
            auth_denials: Vec::new(),
            auth_resets: Vec::new(),
            inserts: HashMap::new(),
            removes: HashMap::new(),
            removes_synthetic: HashMap::new(),
            updates: HashMap::new(),
            empty: true,
        }
    }

    // Public

    /// Returns `true` if no world events are pending.
    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// Drains and returns all events of type `V`.
    pub fn read<V: WorldEvent<E>>(&mut self) -> V::Iter {
        V::iter(self)
    }

    /// Returns `true` if at least one event of type `V` is pending.
    pub fn has<V: WorldEvent<E>>(&self) -> bool {
        V::has(self)
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any incoming messages are queued. Prefer `read::<MessageEvent<C,M>>()`.
    pub fn has_messages(&self) -> bool {
        !self.messages.is_empty()
    }
    /// Drains the raw message map. Prefer `read::<MessageEvent<C,M>>()` over this method.
    pub fn take_messages(&mut self) -> MessagesMap {
        mem::take(&mut self.messages)
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any incoming requests are queued. Prefer `read::<RequestEvent<C,Q>>()`.
    pub fn has_requests(&self) -> bool {
        !self.requests.is_empty()
    }
    /// Drains the raw request map. Prefer `read::<RequestEvent<C,Q>>()` over this method.
    pub fn take_requests(&mut self) -> RequestsMap {
        mem::take(&mut self.requests)
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any component-insert events are pending. Prefer `read::<InsertComponentEvent<C>>()`.
    pub fn has_inserts(&self) -> bool {
        !self.inserts.is_empty()
    }
    /// Drains the raw insert map. Prefer `read::<InsertComponentEvent<C>>()` over this method.
    pub fn take_inserts(&mut self) -> Option<HashMap<ComponentKind, Vec<(UserKey, E)>>> {
        if self.inserts.is_empty() {
            None
        } else {
            Some(mem::take(&mut self.inserts))
        }
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any component-update events are pending. Prefer `read::<UpdateComponentEvent<C>>()`.
    pub fn has_updates(&self) -> bool {
        !self.updates.is_empty()
    }
    /// Drains the raw update map. Prefer `read::<UpdateComponentEvent<C>>()` over this method.
    pub fn take_updates(&mut self) -> Option<HashMap<ComponentKind, Vec<(UserKey, E)>>> {
        if self.updates.is_empty() {
            None
        } else {
            Some(mem::take(&mut self.updates))
        }
    }

    // These method are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any component-remove events are pending. Prefer `read::<RemoveComponentEvent<C>>()`.
    pub fn has_removes(&self) -> bool {
        !self.removes.is_empty()
    }
    /// Drains the raw remove map. Prefer `read::<RemoveComponentEvent<C>>()` over this method.
    pub fn take_removes(&mut self) -> Option<RemovesMap<E>> {
        if self.removes.is_empty() {
            None
        } else {
            Some(mem::take(&mut self.removes))
        }
    }

    /// Drains the kind-only synthetic remove map.
    /// These are fired when an entity is despawned by the client and the server
    /// does not hold component data — only the component kind is available.
    pub fn take_removes_synthetic(&mut self) -> Option<RemovesSyntheticMap<E>> {
        if self.removes_synthetic.is_empty() {
            None
        } else {
            Some(mem::take(&mut self.removes_synthetic))
        }
    }

    // Crate-public

    pub(crate) fn push_connection(&mut self, user_key: &UserKey) {
        self.connections.push(*user_key);
        self.empty = false;
    }

    pub(crate) fn push_disconnection(
        &mut self,
        user_key: &UserKey,
        addr: SocketAddr,
        reason: DisconnectReason,
    ) {
        self.disconnections.push((*user_key, addr, reason));
        self.empty = false;
    }

    pub(crate) fn push_error(&mut self, error: NaiaServerError) {
        self.errors.push(error);
        self.empty = false;
    }

    pub(crate) fn push_message(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        message: MessageContainer,
    ) {
        push_message_impl(&mut self.messages, user_key, channel_kind, message);
        self.empty = false;
    }

    pub(crate) fn push_request(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        global_response_id: GlobalResponseId,
        request: MessageContainer,
    ) {
        if !self.requests.contains_key(channel_kind) {
            self.requests.insert(*channel_kind, HashMap::new());
        }
        let channel_map = self.requests.get_mut(channel_kind).unwrap();
        let request_type_id = request.kind();
        let list = channel_map.entry(request_type_id).or_default();
        list.push((*user_key, global_response_id, request));

        self.empty = false;
    }

    pub(crate) fn push_spawn(&mut self, user_key: &UserKey, world_entity: &E) {
        self.spawns.push((*user_key, *world_entity));
        self.empty = false;
    }

    pub(crate) fn push_despawn(&mut self, user_key: &UserKey, world_entity: &E) {
        self.despawns.push((*user_key, *world_entity));
        self.empty = false;
    }

    pub(crate) fn push_publish(&mut self, user_key: &UserKey, world_entity: &E) {
        self.publishes.push((*user_key, *world_entity));
        self.empty = false;
    }

    pub(crate) fn push_unpublish(&mut self, user_key: &UserKey, world_entity: &E) {
        self.unpublishes.push((*user_key, *world_entity));
        self.empty = false;
    }

    pub(crate) fn push_delegate(&mut self, user_key: &UserKey, world_entity: &E) {
        self.delegates.push((*user_key, *world_entity));
        self.empty = false;
    }

    pub(crate) fn push_auth_grant(&mut self, user_key: &UserKey, world_entity: &E) {
        self.auth_grants.push((*user_key, *world_entity));
        self.empty = false;
    }

    /// Emit when the server rejects a client's authority request (slot already held).
    pub(crate) fn push_auth_denied(&mut self, user_key: &UserKey, world_entity: &E) {
        self.auth_denials.push((*user_key, *world_entity));
        self.empty = false;
    }

    pub(crate) fn push_auth_reset(&mut self, world_entity: &E) {
        self.auth_resets.push(*world_entity);
        self.empty = false;
    }

    pub(crate) fn push_insert(
        &mut self,
        user_key: &UserKey,
        world_entity: &E,
        component_kind: &ComponentKind,
    ) {
        if !self.inserts.contains_key(component_kind) {
            self.inserts.insert(*component_kind, Vec::new());
        }
        let list = self.inserts.get_mut(component_kind).unwrap();
        list.push((*user_key, *world_entity));
        self.empty = false;
    }

    pub(crate) fn push_remove(
        &mut self,
        user_key: &UserKey,
        world_entity: &E,
        component: Box<dyn Replicate>,
    ) {
        let component_kind = component.kind();

        let list = self.removes.entry(component_kind).or_default();
        list.push((*user_key, *world_entity, component));
        self.empty = false;
    }

    /// Fire a kind-only remove event for `component_kind` when the server does not
    /// hold component data (e.g. when a client-owned entity is despawned by the client).
    pub(crate) fn push_remove_synthetic(
        &mut self,
        user_key: &UserKey,
        world_entity: &E,
        component_kind: &ComponentKind,
    ) {
        let list = self.removes_synthetic.entry(*component_kind).or_default();
        list.push((*user_key, *world_entity));
        self.empty = false;
    }

    pub(crate) fn push_update(
        &mut self,
        user_key: &UserKey,
        world_entity: &E,
        component_kind: &ComponentKind,
    ) {
        if !self.updates.contains_key(component_kind) {
            self.updates.insert(*component_kind, Vec::new());
        }
        let list = self.updates.get_mut(component_kind).unwrap();
        list.push((*user_key, *world_entity));
        self.empty = false;
    }
}

impl<E: Hash + Copy + Eq + Sync + Send> Drop for WorldEvents<E> {
    fn drop(&mut self) {
        if !self.spawns.is_empty() {
            warn!("Dropped Server Spawn Event(s)! Make sure to handle these through `events.read::<SpawnEntityEvent>()`, and note that this may be an attack vector.");
        }
        if !self.publishes.is_empty() {
            warn!("Dropped Server Publish Entity Event(s)! Make sure to handle these through `events.read::<PublishEntityEvent>()`, and note that this may be an attack vector.");
        }
        if !self.unpublishes.is_empty() {
            warn!("Dropped Server Unpublish Entity Event(s)! Make sure to handle these through `events.read::<UnpublishEntityEvent>()`, and note that this may be an attack vector.");
        }
        if !self.inserts.is_empty() {
            warn!("Dropped Server Insert Event(s)! Make sure to handle these through `events.read::<InsertComponentEvent<Component>>()`, and note that this may be an attack vector.");
        }
        if !self.updates.is_empty() {
            warn!("Dropped Server Update Event(s)! Make sure to handle these through `events.read::<UpdateComponentEvent<Component>>()`, and note that this may be an attack vector.");
        }
    }
}

/// Marker trait for types that can be read from [`WorldEvents<E>`].
pub trait WorldEvent<E: Hash + Copy + Eq + Sync + Send> {
    /// Iterator type yielded by [`WorldEvents::read`].
    type Iter;

    /// Drains all events of this type and returns an iterator.
    fn iter(events: &mut WorldEvents<E>) -> Self::Iter;

    /// Returns `true` if at least one event of this type is pending.
    fn has(events: &WorldEvents<E>) -> bool;
}

// ConnectEvent
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for ConnectEvent {
    type Iter = IntoIter<UserKey>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.connections);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.connections.is_empty()
    }
}

// DisconnectEvent
/// Fires when a client disconnects or times out; yields `(UserKey, SocketAddr, DisconnectReason)`.
pub struct DisconnectEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for DisconnectEvent {
    type Iter = IntoIter<(UserKey, SocketAddr, DisconnectReason)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.disconnections);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.disconnections.is_empty()
    }
}

// Error Event
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for ErrorEvent {
    type Iter = IntoIter<NaiaServerError>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.errors);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.errors.is_empty()
    }
}

// Message Event
/// Fires when a client sends a message `M` over channel `C`; yields `(UserKey, M)` pairs.
pub struct MessageEvent<C: Channel, M: Message> {
    phantom_c: PhantomData<C>,
    phantom_m: PhantomData<M>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Channel, M: Message> WorldEvent<E>
    for MessageEvent<C, M>
{
    type Iter = IntoIter<(UserKey, M)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let output = read_channel_messages::<C, M>(&mut events.messages);
        IntoIterator::into_iter(output)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        if let Some(channel_map) = events.messages.get(&channel_kind) {
            let message_kind: MessageKind = MessageKind::of::<M>();
            return channel_map.contains_key(&message_kind);
        }
        false
    }
}

pub(crate) fn read_channel_messages<C: Channel, M: Message>(
    messages: &mut MessagesMap,
) -> Vec<(UserKey, M)> {
    let channel_kind: ChannelKind = ChannelKind::of::<C>();
    if let Some(channel_map) = messages.get_mut(&channel_kind) {
        let message_kind: MessageKind = MessageKind::of::<M>();
        if let Some(messages) = channel_map.remove(&message_kind) {
            return read_messages(messages);
        }
    }

    Vec::new()
}

pub(crate) fn read_messages<M: Message>(
    messages: Vec<(UserKey, MessageContainer)>,
) -> Vec<(UserKey, M)> {
    let mut output_list: Vec<(UserKey, M)> = Vec::new();

    for (user_key, message) in messages {
        let message: M = Box::<dyn Any + 'static>::downcast::<M>(message.to_boxed_any())
            .ok()
            .map(|boxed_m| *boxed_m)
            .unwrap();
        output_list.push((user_key, message));
    }

    output_list
}

pub(crate) fn push_message_impl(
    messages: &mut MessagesMap,
    user_key: &UserKey,
    channel_kind: &ChannelKind,
    message: MessageContainer,
) {
    if !messages.contains_key(channel_kind) {
        messages.insert(*channel_kind, HashMap::new());
    }
    let channel_map = messages.get_mut(channel_kind).unwrap();
    let message_type_id = message.kind();
    let list = channel_map.entry(message_type_id).or_default();
    list.push((*user_key, message));
}

// Request Event
/// Fires when a client sends a request `Q` over channel `C`; yields `(UserKey, ResponseSendKey<Q::Response>, Q)`.
pub struct RequestEvent<C: Channel, Q: Request> {
    phantom_c: PhantomData<C>,
    phantom_m: PhantomData<Q>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Channel, Q: Request> WorldEvent<E>
    for RequestEvent<C, Q>
{
    type Iter = IntoIter<(UserKey, ResponseSendKey<Q::Response>, Q)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        let Some(channel_map) = events.requests.get_mut(&channel_kind) else {
            return IntoIterator::into_iter(Vec::new());
        };
        let message_kind: MessageKind = MessageKind::of::<Q>();
        let Some(requests) = channel_map.remove(&message_kind) else {
            return IntoIterator::into_iter(Vec::new());
        };

        let mut output_list = Vec::new();

        for (user_key, global_response_id, request) in requests {
            let request: Q = Box::<dyn Any + 'static>::downcast::<Q>(request.to_boxed_any())
                .ok()
                .map(|boxed_q| *boxed_q)
                .unwrap();
            let response_send_key = ResponseSendKey::<Q::Response>::new(global_response_id);
            output_list.push((user_key, response_send_key, request));
        }

        IntoIterator::into_iter(output_list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        if let Some(channel_map) = events.requests.get(&channel_kind) {
            let message_kind: MessageKind = MessageKind::of::<Q>();
            return channel_map.contains_key(&message_kind);
        }
        false
    }
}

// Spawn Entity Event
/// Fires when a client-authoritative entity enters the server's scope; yields `(UserKey, E)`.
pub struct SpawnEntityEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for SpawnEntityEvent {
    type Iter = IntoIter<(UserKey, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.spawns);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.spawns.is_empty()
    }
}

// Despawn Entity Event
/// Fires when an entity is removed from the server's world; yields `(UserKey, E)`.
pub struct DespawnEntityEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for DespawnEntityEvent {
    type Iter = IntoIter<(UserKey, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.despawns);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.despawns.is_empty()
    }
}

// Publish Entity Event
/// Fires when a private entity is published (made visible to in-scope clients); yields `(UserKey, E)`.
pub struct PublishEntityEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for PublishEntityEvent {
    type Iter = IntoIter<(UserKey, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.publishes);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.publishes.is_empty()
    }
}

// Unpublish Entity Event
/// Fires when a public entity is unpublished (hidden from all clients); yields `(UserKey, E)`.
pub struct UnpublishEntityEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for UnpublishEntityEvent {
    type Iter = IntoIter<(UserKey, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.unpublishes);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.unpublishes.is_empty()
    }
}

// Delegate Entity Event
/// Fires when an entity's replication mode is switched to `Delegated`; yields `(UserKey, E)`.
pub struct DelegateEntityEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for DelegateEntityEvent {
    type Iter = IntoIter<(UserKey, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.delegates);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.delegates.is_empty()
    }
}

// Entity Auth Given Event
/// Fires when the server grants a client authority over a delegated entity; yields `(UserKey, E)`.
pub struct EntityAuthGrantEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for EntityAuthGrantEvent {
    type Iter = IntoIter<(UserKey, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.auth_grants);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.auth_grants.is_empty()
    }
}

// Entity Auth Denied Event
/// Emitted when the server rejects a client's `RequestAuthority` because
/// another user already holds the entity's authority slot.
pub struct EntityAuthDeniedEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for EntityAuthDeniedEvent {
    type Iter = IntoIter<(UserKey, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.auth_denials);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.auth_denials.is_empty()
    }
}

// Entity Auth Reset Event
/// Fires when entity authority is released back to the server; yields the entity `E`.
pub struct EntityAuthResetEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for EntityAuthResetEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.auth_resets);
        IntoIterator::into_iter(list)
    }

    fn has(events: &WorldEvents<E>) -> bool {
        !events.auth_resets.is_empty()
    }
}

// Insert Component Event
/// Fires when component `C` is inserted on an in-scope entity; yields `(UserKey, E)`.
pub struct InsertComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Replicate> WorldEvent<E> for InsertComponentEvent<C> {
    type Iter = IntoIter<(UserKey, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.inserts.remove(&component_kind) {
            return IntoIterator::into_iter(boxed_list);
        }

        IntoIterator::into_iter(Vec::new())
    }

    fn has(events: &WorldEvents<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.inserts.contains_key(&component_kind)
    }
}

// Update Component Event
/// Fires when component `C` is mutated on an in-scope entity; yields `(UserKey, E)`.
pub struct UpdateComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Replicate> WorldEvent<E> for UpdateComponentEvent<C> {
    type Iter = IntoIter<(UserKey, E)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.updates.remove(&component_kind) {
            return IntoIterator::into_iter(boxed_list);
        }

        IntoIterator::into_iter(Vec::new())
    }

    fn has(events: &WorldEvents<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.updates.contains_key(&component_kind)
    }
}

// Remove Component Event
/// Fires when component `C` is removed from an entity; yields `(UserKey, E, C)` with the removed value.
pub struct RemoveComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Replicate> WorldEvent<E> for RemoveComponentEvent<C> {
    type Iter = IntoIter<(UserKey, E, C)>;

    fn iter(events: &mut WorldEvents<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.removes.remove(&component_kind) {
            let mut output_list: Vec<(UserKey, E, C)> = Vec::new();

            for (user_key, entity, boxed_component) in boxed_list {
                let boxed_any = boxed_component.to_boxed_any();
                let component = boxed_any.downcast::<C>().unwrap();
                output_list.push((user_key, entity, *component));
            }

            return IntoIterator::into_iter(output_list);
        }

        IntoIterator::into_iter(Vec::new())
    }

    fn has(events: &WorldEvents<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.removes.contains_key(&component_kind)
    }
}
