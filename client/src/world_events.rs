use std::{
    collections::HashMap, hash::Hash, marker::PhantomData, mem, net::SocketAddr, vec::IntoIter,
};

use naia_shared::{
    handshake::RejectReason, Channel, ChannelKind, ComponentKind, DisconnectReason,
    GlobalResponseId, Message, MessageContainer, MessageKind, Replicate, Request, ResponseSendKey, Tick,
};

use crate::NaiaClientError;

type RemovesMap<E> = HashMap<ComponentKind, Vec<(E, Box<dyn Replicate>)>>;

/// All events produced in one frame: connections, entity lifecycle, component changes, messages, and errors.
pub struct Events<E: Hash + Copy + Eq + Sync + Send> {
    connections: Vec<SocketAddr>,
    rejections: Vec<(SocketAddr, RejectReason)>,
    disconnections: Vec<(SocketAddr, DisconnectReason)>,
    errors: Vec<NaiaClientError>,
    messages: HashMap<ChannelKind, HashMap<MessageKind, Vec<MessageContainer>>>,
    requests: HashMap<ChannelKind, HashMap<MessageKind, Vec<(GlobalResponseId, MessageContainer)>>>,
    spawns: Vec<E>,
    despawns: Vec<E>,
    publishes: Vec<E>,
    unpublishes: Vec<E>,
    auth_grants: Vec<E>,
    auth_denies: Vec<E>,
    auth_resets: Vec<E>,
    inserts: HashMap<ComponentKind, Vec<E>>,
    removes: RemovesMap<E>,
    updates: HashMap<ComponentKind, Vec<(Tick, E)>>,
    empty: bool,
}

impl<E: Hash + Copy + Eq + Sync + Send> Default for Events<E> {
    fn default() -> Self {
        Events::new()
    }
}

impl<E: Hash + Copy + Eq + Sync + Send> Events<E> {
    pub(crate) fn new() -> Self {
        Self {
            connections: Vec::new(),
            rejections: Vec::new(),
            disconnections: Vec::new(), // (SocketAddr, DisconnectReason)
            errors: Vec::new(),
            messages: HashMap::new(),
            requests: HashMap::new(),
            spawns: Vec::new(),
            despawns: Vec::new(),
            publishes: Vec::new(),
            unpublishes: Vec::new(),
            auth_grants: Vec::new(),
            auth_denies: Vec::new(),
            auth_resets: Vec::new(),
            inserts: HashMap::new(),
            removes: HashMap::new(),
            updates: HashMap::new(),
            empty: true,
        }
    }

    /// Returns `true` if no events were queued this frame.
    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// Drains and returns an iterator over events of type `V`.
    pub fn read<V: WorldEvent<E>>(&mut self) -> V::Iter {
        V::iter(self)
    }

    /// Returns `true` if at least one event of type `V` is queued.
    pub fn has<V: WorldEvent<E>>(&self) -> bool {
        V::has(self)
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any messages are queued; prefer `read::<MessageEvent<C, M>>()` in application code.
    pub fn has_messages(&self) -> bool {
        !self.messages.is_empty()
    }
    /// Takes all queued messages, leaving the internal buffer empty; prefer `read::<MessageEvent<C, M>>()` in application code.
    pub fn take_messages(
        &mut self,
    ) -> HashMap<ChannelKind, HashMap<MessageKind, Vec<MessageContainer>>> {
        mem::take(&mut self.messages)
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any requests are queued; prefer `read::<RequestEvent<C, Q>>()` in application code.
    pub fn has_requests(&self) -> bool {
        !self.requests.is_empty()
    }
    /// Takes all queued requests, leaving the internal buffer empty; prefer `read::<RequestEvent<C, Q>>()` in application code.
    pub fn take_requests(
        &mut self,
    ) -> HashMap<ChannelKind, HashMap<MessageKind, Vec<(GlobalResponseId, MessageContainer)>>> {
        mem::take(&mut self.requests)
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any component-insert events are queued; prefer `read::<InsertComponentEvent<C>>()` in application code.
    pub fn has_inserts(&self) -> bool {
        !self.inserts.is_empty()
    }
    /// Takes all queued component-insert events; prefer `read::<InsertComponentEvent<C>>()` in application code.
    pub fn take_inserts(&mut self) -> Option<HashMap<ComponentKind, Vec<E>>> {
        if self.inserts.is_empty() {
            None
        } else {
            Some(mem::take(&mut self.inserts))
        }
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any component-update events are queued; prefer `read::<UpdateComponentEvent<C>>()` in application code.
    pub fn has_updates(&self) -> bool {
        !self.updates.is_empty()
    }
    /// Takes all queued component-update events; prefer `read::<UpdateComponentEvent<C>>()` in application code.
    pub fn take_updates(&mut self) -> Option<HashMap<ComponentKind, Vec<(Tick, E)>>> {
        if self.updates.is_empty() {
            None
        } else {
            Some(mem::take(&mut self.updates))
        }
    }

    // These method are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    /// Returns `true` if any component-remove events are queued; prefer `read::<RemoveComponentEvent<C>>()` in application code.
    pub fn has_removes(&self) -> bool {
        !self.removes.is_empty()
    }
    /// Takes all queued component-remove events; prefer `read::<RemoveComponentEvent<C>>()` in application code.
    pub fn take_removes(&mut self) -> Option<RemovesMap<E>> {
        if self.removes.is_empty() {
            None
        } else {
            Some(mem::take(&mut self.removes))
        }
    }

    // Crate-public

    pub(crate) fn push_connection(&mut self, socket_addr: &SocketAddr) {
        self.connections.push(*socket_addr);
        self.empty = false;
    }

    pub(crate) fn push_rejection(&mut self, socket_addr: &SocketAddr, reason: RejectReason) {
        self.rejections.push((*socket_addr, reason));
        self.empty = false;
    }

    pub(crate) fn push_disconnection(&mut self, socket_addr: &SocketAddr, reason: DisconnectReason) {
        self.disconnections.push((*socket_addr, reason));
        self.empty = false;
    }

    pub(crate) fn push_message(&mut self, channel_kind: &ChannelKind, message: MessageContainer) {
        if !self.messages.contains_key(channel_kind) {
            self.messages.insert(*channel_kind, HashMap::new());
        }
        let channel_map = self.messages.get_mut(channel_kind).unwrap();

        let message_kind: MessageKind = message.kind();
        channel_map.entry(message_kind).or_default();
        let list = channel_map.get_mut(&message_kind).unwrap();
        list.push(message);
        self.empty = false;
    }

    pub(crate) fn push_request(
        &mut self,
        channel_kind: &ChannelKind,
        global_response_id: GlobalResponseId,
        request: MessageContainer,
    ) {
        if !self.requests.contains_key(channel_kind) {
            self.requests.insert(*channel_kind, HashMap::new());
        }
        let channel_map = self.requests.get_mut(channel_kind).unwrap();

        let message_kind: MessageKind = request.kind();
        channel_map.entry(message_kind).or_default();
        let list = channel_map.get_mut(&message_kind).unwrap();
        list.push((global_response_id, request));

        self.empty = false;
    }

    pub(crate) fn push_error(&mut self, error: NaiaClientError) {
        self.errors.push(error);
        self.empty = false;
    }

    pub(crate) fn push_spawn(&mut self, world_entity: E) {
        self.spawns.push(world_entity);
        self.empty = false;
    }

    pub(crate) fn push_despawn(&mut self, world_entity: E) {
        self.despawns.push(world_entity);
        self.empty = false;
    }

    pub(crate) fn push_publish(&mut self, world_entity: E) {
        self.publishes.push(world_entity);
        self.empty = false;
    }

    pub(crate) fn push_unpublish(&mut self, world_entity: E) {
        self.unpublishes.push(world_entity);
        self.empty = false;
    }

    pub(crate) fn push_auth_grant(&mut self, world_entity: E) {
        self.auth_grants.push(world_entity);
        self.empty = false;
    }

    pub(crate) fn push_auth_deny(&mut self, world_entity: E) {
        self.auth_denies.push(world_entity);
        self.empty = false;
    }

    pub(crate) fn push_auth_reset(&mut self, world_entity: E) {
        self.auth_resets.push(world_entity);
        self.empty = false;
    }

    pub(crate) fn push_insert(&mut self, world_entity: E, component_kind: ComponentKind) {
        self.inserts.entry(component_kind).or_insert_with(|| Vec::new());
        let list = self.inserts.get_mut(&component_kind).unwrap();
        list.push(world_entity);
        self.empty = false;
    }

    pub(crate) fn push_update(
        &mut self,
        tick: Tick,
        world_entity: E,
        component_kind: ComponentKind,
    ) {
        self.updates.entry(component_kind).or_default();
        let list = self.updates.get_mut(&component_kind).unwrap();
        list.push((tick, world_entity));
        self.empty = false;
    }

    pub(crate) fn push_remove(&mut self, world_entity: E, component: Box<dyn Replicate>) {
        let component_kind: ComponentKind = component.kind();
        self.removes.entry(component_kind).or_default();
        let list = self.removes.get_mut(&component_kind).unwrap();
        list.push((world_entity, component));
        self.empty = false;
    }

    pub(crate) fn clear(&mut self) {
        self.connections.clear();
        self.rejections.clear();
        self.disconnections.clear();
        self.errors.clear();
        self.messages.clear();
        self.requests.clear();
        self.spawns.clear();
        self.despawns.clear();
        self.publishes.clear();
        self.unpublishes.clear();
        self.auth_grants.clear();
        self.auth_denies.clear();
        self.auth_resets.clear();
        self.inserts.clear();
        self.removes.clear();
        self.updates.clear();
        self.empty = true;
    }
}

/// Type-indexed world event; each concrete type selects one category from [`Events`].
pub trait WorldEvent<E: Hash + Copy + Eq + Sync + Send> {
    /// Iterator type returned from [`Events::read`].
    type Iter;

    /// Drains events of this variant out of `events` and returns an iterator over them.
    fn iter(events: &mut Events<E>) -> Self::Iter;

    /// Returns `true` if `events` contains at least one event of this variant.
    fn has(events: &Events<E>) -> bool;
}

/// Fires when the client successfully establishes a connection to the server; yields the server's [`SocketAddr`].
pub struct ConnectEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for ConnectEvent {
    type Iter = IntoIter<SocketAddr>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.connections);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.connections.is_empty()
    }
}

/// Fires when the server explicitly rejects the connection; yields the server address and the [`RejectReason`].
pub struct RejectEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for RejectEvent {
    type Iter = IntoIter<(SocketAddr, RejectReason)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.rejections);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.rejections.is_empty()
    }
}

/// Fires when the connection to the server is lost; yields the server address and the [`DisconnectReason`].
pub struct DisconnectEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for DisconnectEvent {
    type Iter = IntoIter<(SocketAddr, DisconnectReason)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.disconnections);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.disconnections.is_empty()
    }
}

/// Fires when a transport or protocol error occurs; yields a [`NaiaClientError`].
pub struct ErrorEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for ErrorEvent {
    type Iter = IntoIter<NaiaClientError>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.errors);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.errors.is_empty()
    }
}

/// Fires when a message of type `M` arrives on channel `C`; yields the decoded `M` value.
pub struct MessageEvent<C: Channel, M: Message> {
    phantom_c: PhantomData<C>,
    phantom_m: PhantomData<M>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Channel, M: Message> WorldEvent<E>
    for MessageEvent<C, M>
{
    type Iter = IntoIter<M>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        if let Some(channel_map) = events.messages.get_mut(&channel_kind) {
            let message_kind: MessageKind = MessageKind::of::<M>();
            if let Some(boxed_list) = channel_map.remove(&message_kind) {
                let mut output_list: Vec<M> = Vec::new();

                for boxed_message in boxed_list {
                    let boxed_any = boxed_message.to_boxed_any();
                    let message = boxed_any.downcast::<M>().unwrap();
                    output_list.push(*message);
                }

                return IntoIterator::into_iter(output_list);
            }
        }
        IntoIterator::into_iter(Vec::new())
    }

    fn has(events: &Events<E>) -> bool {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        if let Some(channel_map) = events.messages.get(&channel_kind) {
            let message_kind: MessageKind = MessageKind::of::<M>();
            return channel_map.contains_key(&message_kind);
        }
        false
    }
}

/// Fires when a request of type `Q` arrives on channel `C`; yields a `(ResponseSendKey, Q)` pair.
pub struct RequestEvent<C: Channel, Q: Request> {
    phantom_c: PhantomData<C>,
    phantom_m: PhantomData<Q>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Channel, Q: Request> WorldEvent<E>
    for RequestEvent<C, Q>
{
    type Iter = IntoIter<(ResponseSendKey<Q::Response>, Q)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        let Some(channel_map) = events.requests.get_mut(&channel_kind) else {
            return IntoIterator::into_iter(Vec::new());
        };
        let message_kind: MessageKind = MessageKind::of::<Q>();
        let Some(requests) = channel_map.remove(&message_kind) else {
            return IntoIterator::into_iter(Vec::new());
        };
        let mut output_list = Vec::new();

        for (global_response_id, boxed_request) in requests {
            let boxed_any = boxed_request.to_boxed_any();
            let request = boxed_any.downcast::<Q>().unwrap();
            let response_send_key = ResponseSendKey::<Q::Response>::new(global_response_id);
            output_list.push((response_send_key, *request));
        }

        IntoIterator::into_iter(output_list)
    }

    fn has(events: &Events<E>) -> bool {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        if let Some(channel_map) = events.requests.get(&channel_kind) {
            let message_kind: MessageKind = MessageKind::of::<Q>();
            return channel_map.contains_key(&message_kind);
        }
        false
    }
}

/// Fires when the server spawns a new replicated entity on this client; yields the world entity `E`.
pub struct SpawnEntityEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for SpawnEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.spawns);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.spawns.is_empty()
    }
}

/// Fires when the server despawns a previously replicated entity; yields the world entity `E`.
pub struct DespawnEntityEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for DespawnEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.despawns);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.despawns.is_empty()
    }
}

/// Fires when an entity transitions to the `Public` visibility state and becomes visible to all users.
pub struct PublishEntityEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for PublishEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.publishes);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.publishes.is_empty()
    }
}

/// Fires when an entity's visibility is retracted from the `Public` state.
pub struct UnpublishEntityEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for UnpublishEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.unpublishes);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.unpublishes.is_empty()
    }
}

/// Fires when the server grants this client authority over a delegated entity.
pub struct EntityAuthGrantedEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for EntityAuthGrantedEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.auth_grants);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.auth_grants.is_empty()
    }
}

/// Fires when the server reclaims authority over an entity that was previously delegated to this client.
pub struct EntityAuthResetEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for EntityAuthResetEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.auth_resets);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.auth_resets.is_empty()
    }
}

/// Fires when the server denies this client's authority request for a delegated entity.
pub struct EntityAuthDeniedEvent;
impl<E: Hash + Copy + Eq + Sync + Send> WorldEvent<E> for EntityAuthDeniedEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.auth_denies);
        IntoIterator::into_iter(list)
    }

    fn has(events: &Events<E>) -> bool {
        !events.auth_denies.is_empty()
    }
}

/// Fires when component `C` is inserted on a replicated entity; yields the world entity `E`.
pub struct InsertComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Replicate> WorldEvent<E> for InsertComponentEvent<C> {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.inserts.remove(&component_kind) {
            return IntoIterator::into_iter(boxed_list);
        }

        IntoIterator::into_iter(Vec::new())
    }

    fn has(events: &Events<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.inserts.contains_key(&component_kind)
    }
}

/// Fires when component `C` on a replicated entity is mutated; yields `(Tick, E)`.
pub struct UpdateComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Replicate> WorldEvent<E> for UpdateComponentEvent<C> {
    type Iter = IntoIter<(Tick, E)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.updates.remove(&component_kind) {
            return IntoIterator::into_iter(boxed_list);
        }

        IntoIterator::into_iter(Vec::new())
    }

    fn has(events: &Events<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.updates.contains_key(&component_kind)
    }
}

/// Fires when component `C` is removed from a replicated entity; yields `(E, C)` with the last value of the component.
pub struct RemoveComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Hash + Copy + Eq + Sync + Send, C: Replicate> WorldEvent<E> for RemoveComponentEvent<C> {
    type Iter = IntoIter<(E, C)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.removes.remove(&component_kind) {
            let mut output_list: Vec<(E, C)> = Vec::new();

            for (entity, boxed_component) in boxed_list {
                let boxed_any = boxed_component.to_boxed_any();
                let component = boxed_any.downcast::<C>().unwrap();
                output_list.push((entity, *component));
            }

            return IntoIterator::into_iter(output_list);
        }

        IntoIterator::into_iter(Vec::new())
    }

    fn has(events: &Events<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.removes.contains_key(&component_kind)
    }
}
