use std::{collections::HashMap, marker::PhantomData, mem, net::SocketAddr, vec::IntoIter};

use naia_shared::{Channel, ChannelKind, ComponentKind, EntityEvent, EntityResponseEvent, GlobalResponseId, Message, MessageContainer, MessageKind, Replicate, Request, ResponseSendKey, Tick};

use crate::NaiaClientError;

pub struct Events<E: Copy> {
    connections: Vec<SocketAddr>,
    rejections: Vec<SocketAddr>,
    disconnections: Vec<SocketAddr>,
    client_ticks: Vec<Tick>,
    server_ticks: Vec<Tick>,
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
    removes: HashMap<ComponentKind, Vec<(E, Box<dyn Replicate>)>>,
    updates: HashMap<ComponentKind, Vec<(Tick, E)>>,
    empty: bool,
}

impl<E: Copy> Default for Events<E> {
    fn default() -> Self {
        Events::new()
    }
}

impl<E: Copy> Events<E> {
    pub(crate) fn new() -> Self {
        Self {
            connections: Vec::new(),
            rejections: Vec::new(),
            disconnections: Vec::new(),
            client_ticks: Vec::new(),
            server_ticks: Vec::new(),
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

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn read<V: Event<E>>(&mut self) -> V::Iter {
        return V::iter(self);
    }

    pub fn has<V: Event<E>>(&self) -> bool {
        return V::has(self);
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_messages(&self) -> bool {
        !self.messages.is_empty()
    }
    pub fn take_messages(
        &mut self,
    ) -> HashMap<ChannelKind, HashMap<MessageKind, Vec<MessageContainer>>> {
        mem::take(&mut self.messages)
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_requests(&self) -> bool {
        !self.requests.is_empty()
    }
    pub fn take_requests(
        &mut self,
    ) -> HashMap<ChannelKind, HashMap<MessageKind, Vec<(GlobalResponseId, MessageContainer)>>> {
        mem::take(&mut self.requests)
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_inserts(&self) -> bool {
        !self.inserts.is_empty()
    }
    pub fn take_inserts(&mut self) -> Option<HashMap<ComponentKind, Vec<E>>> {
        if self.inserts.is_empty() {
            return None;
        } else {
            return Some(mem::take(&mut self.inserts));
        }
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_updates(&self) -> bool {
        !self.updates.is_empty()
    }
    pub fn take_updates(&mut self) -> Option<HashMap<ComponentKind, Vec<(Tick, E)>>> {
        if self.updates.is_empty() {
            return None;
        } else {
            return Some(mem::take(&mut self.updates));
        }
    }

    // These method are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_removes(&self) -> bool {
        !self.removes.is_empty()
    }
    pub fn take_removes(&mut self) -> Option<HashMap<ComponentKind, Vec<(E, Box<dyn Replicate>)>>> {
        if self.removes.is_empty() {
            return None;
        } else {
            return Some(mem::take(&mut self.removes));
        }
    }

    // Crate-public

    pub(crate) fn push_connection(&mut self, socket_addr: &SocketAddr) {
        self.connections.push(*socket_addr);
        self.empty = false;
    }

    pub(crate) fn push_rejection(&mut self, socket_addr: &SocketAddr) {
        self.rejections.push(*socket_addr);
        self.empty = false;
    }

    pub(crate) fn push_disconnection(&mut self, socket_addr: &SocketAddr) {
        self.disconnections.push(*socket_addr);
        self.empty = false;
    }

    pub(crate) fn push_message(&mut self, channel_kind: &ChannelKind, message: MessageContainer) {
        if !self.messages.contains_key(&channel_kind) {
            self.messages.insert(*channel_kind, HashMap::new());
        }
        let channel_map = self.messages.get_mut(&channel_kind).unwrap();

        let message_kind: MessageKind = message.kind();
        if !channel_map.contains_key(&message_kind) {
            channel_map.insert(message_kind, Vec::new());
        }
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
        if !self.requests.contains_key(&channel_kind) {
            self.requests.insert(*channel_kind, HashMap::new());
        }
        let channel_map = self.requests.get_mut(&channel_kind).unwrap();

        let message_kind: MessageKind = request.kind();
        if !channel_map.contains_key(&message_kind) {
            channel_map.insert(message_kind, Vec::new());
        }
        let list = channel_map.get_mut(&message_kind).unwrap();
        list.push((global_response_id, request));

        self.empty = false;
    }

    pub(crate) fn push_client_tick(&mut self, tick: Tick) {
        self.client_ticks.push(tick);
        self.empty = false;
    }

    pub(crate) fn push_server_tick(&mut self, tick: Tick) {
        self.server_ticks.push(tick);
        self.empty = false;
    }

    pub(crate) fn push_error(&mut self, error: NaiaClientError) {
        self.errors.push(error);
        self.empty = false;
    }

    pub(crate) fn push_spawn(&mut self, entity: E) {
        self.spawns.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_despawn(&mut self, entity: E) {
        self.despawns.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_publish(&mut self, entity: E) {
        self.publishes.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_unpublish(&mut self, entity: E) {
        self.unpublishes.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_auth_grant(&mut self, entity: E) {
        self.auth_grants.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_auth_deny(&mut self, entity: E) {
        self.auth_denies.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_auth_reset(&mut self, entity: E) {
        self.auth_resets.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_insert(&mut self, entity: E, component_kind: ComponentKind) {
        if !self.inserts.contains_key(&component_kind) {
            self.inserts.insert(component_kind, Vec::new());
        }
        let list = self.inserts.get_mut(&component_kind).unwrap();
        list.push(entity);
        self.empty = false;
    }

    pub(crate) fn push_update(&mut self, tick: Tick, entity: E, component_kind: ComponentKind) {
        if !self.updates.contains_key(&component_kind) {
            self.updates.insert(component_kind, Vec::new());
        }
        let list = self.updates.get_mut(&component_kind).unwrap();
        list.push((tick, entity));
        self.empty = false;
    }

    pub(crate) fn push_remove(&mut self, entity: E, component: Box<dyn Replicate>) {
        let component_kind: ComponentKind = component.kind();
        if !self.removes.contains_key(&component_kind) {
            self.removes.insert(component_kind, Vec::new());
        }
        let list = self.removes.get_mut(&component_kind).unwrap();
        list.push((entity, component));
        self.empty = false;
    }

    pub(crate) fn receive_world_events(
        &mut self,
        entity_events: Vec<EntityEvent<E>>,
    ) -> Vec<EntityResponseEvent<E>> {
        let mut response_events = Vec::new();
        for event in entity_events {
            match event {
                EntityEvent::SpawnEntity(entity) => {
                    self.push_spawn(entity);
                    response_events.push(EntityResponseEvent::SpawnEntity(entity));
                }
                EntityEvent::DespawnEntity(entity) => {
                    self.push_despawn(entity);
                    response_events.push(EntityResponseEvent::DespawnEntity(entity));
                }
                EntityEvent::InsertComponent(entity, component_kind) => {
                    self.push_insert(entity, component_kind);
                    response_events
                        .push(EntityResponseEvent::InsertComponent(entity, component_kind));
                }
                EntityEvent::RemoveComponent(entity, component_box) => {
                    let kind = component_box.kind();
                    self.push_remove(entity, component_box);
                    response_events.push(EntityResponseEvent::RemoveComponent(entity, kind));
                }
                EntityEvent::UpdateComponent(tick, entity, component_kind) => {
                    self.push_update(tick, entity, component_kind);
                }
            }
        }
        response_events
    }

    pub(crate) fn clear(&mut self) {
        self.connections.clear();
        self.rejections.clear();
        self.disconnections.clear();
        self.client_ticks.clear();
        self.server_ticks.clear();
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

// Event Trait
pub trait Event<E: Copy> {
    type Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter;

    fn has(events: &Events<E>) -> bool;
}

// ConnectEvent
pub struct ConnectEvent;
impl<E: Copy> Event<E> for ConnectEvent {
    type Iter = IntoIter<SocketAddr>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.connections);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.connections.is_empty()
    }
}

// RejectEvent
pub struct RejectEvent;
impl<E: Copy> Event<E> for RejectEvent {
    type Iter = IntoIter<SocketAddr>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.rejections);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.rejections.is_empty()
    }
}

// DisconnectEvent
pub struct DisconnectEvent;
impl<E: Copy> Event<E> for DisconnectEvent {
    type Iter = IntoIter<SocketAddr>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.disconnections);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.disconnections.is_empty()
    }
}

// Client Tick Event
pub struct ClientTickEvent;
impl<E: Copy> Event<E> for ClientTickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.client_ticks);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.client_ticks.is_empty()
    }
}

// Server Tick Event
pub struct ServerTickEvent;
impl<E: Copy> Event<E> for ServerTickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.server_ticks);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.server_ticks.is_empty()
    }
}

// Error Event
pub struct ErrorEvent;
impl<E: Copy> Event<E> for ErrorEvent {
    type Iter = IntoIter<NaiaClientError>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.errors);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.errors.is_empty()
    }
}

// Message Event
pub struct MessageEvent<C: Channel, M: Message> {
    phantom_c: PhantomData<C>,
    phantom_m: PhantomData<M>,
}
impl<E: Copy, C: Channel, M: Message> Event<E> for MessageEvent<C, M> {
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
        return IntoIterator::into_iter(Vec::new());
    }

    fn has(events: &Events<E>) -> bool {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        if let Some(channel_map) = events.messages.get(&channel_kind) {
            let message_kind: MessageKind = MessageKind::of::<M>();
            return channel_map.contains_key(&message_kind);
        }
        return false;
    }
}

// Request Event
pub struct RequestEvent<C: Channel, Q: Request> {
    phantom_c: PhantomData<C>,
    phantom_m: PhantomData<Q>,
}
impl<E: Copy, C: Channel, Q: Request> Event<E> for RequestEvent<C, Q> {
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

        return IntoIterator::into_iter(output_list);
    }

    fn has(events: &Events<E>) -> bool {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        if let Some(channel_map) = events.requests.get(&channel_kind) {
            let message_kind: MessageKind = MessageKind::of::<Q>();
            return channel_map.contains_key(&message_kind);
        }
        return false;
    }
}

// Spawn Entity Event
pub struct SpawnEntityEvent;
impl<E: Copy> Event<E> for SpawnEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.spawns);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.spawns.is_empty()
    }
}

// Despawn Entity Event
pub struct DespawnEntityEvent;
impl<E: Copy> Event<E> for DespawnEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.despawns);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.despawns.is_empty()
    }
}

// Publish Entity Event
pub struct PublishEntityEvent;
impl<E: Copy> Event<E> for PublishEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.publishes);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.publishes.is_empty()
    }
}

// Unpublish Entity Event
pub struct UnpublishEntityEvent;
impl<E: Copy> Event<E> for UnpublishEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.unpublishes);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.unpublishes.is_empty()
    }
}

// Auth Grant Entity Event
pub struct EntityAuthGrantedEvent;
impl<E: Copy> Event<E> for EntityAuthGrantedEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.auth_grants);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.auth_grants.is_empty()
    }
}

// Auth Reset Entity Event
pub struct EntityAuthResetEvent;
impl<E: Copy> Event<E> for EntityAuthResetEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.auth_resets);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.auth_resets.is_empty()
    }
}

// Auth Deny Entity Event
pub struct EntityAuthDeniedEvent;
impl<E: Copy> Event<E> for EntityAuthDeniedEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.auth_denies);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &Events<E>) -> bool {
        !events.auth_denies.is_empty()
    }
}

// Insert Component Event
pub struct InsertComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Copy, C: Replicate> Event<E> for InsertComponentEvent<C> {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.inserts.remove(&component_kind) {
            return IntoIterator::into_iter(boxed_list);
        }

        return IntoIterator::into_iter(Vec::new());
    }

    fn has(events: &Events<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.inserts.contains_key(&component_kind)
    }
}

// Update Component Event
pub struct UpdateComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Copy, C: Replicate> Event<E> for UpdateComponentEvent<C> {
    type Iter = IntoIter<(Tick, E)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        if let Some(boxed_list) = events.updates.remove(&component_kind) {
            return IntoIterator::into_iter(boxed_list);
        }

        return IntoIterator::into_iter(Vec::new());
    }

    fn has(events: &Events<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.updates.contains_key(&component_kind)
    }
}

// Remove Component Event
pub struct RemoveComponentEvent<C: Replicate> {
    phantom_c: PhantomData<C>,
}
impl<E: Copy, C: Replicate> Event<E> for RemoveComponentEvent<C> {
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

        return IntoIterator::into_iter(Vec::new());
    }

    fn has(events: &Events<E>) -> bool {
        let component_kind: ComponentKind = ComponentKind::of::<C>();
        events.removes.contains_key(&component_kind)
    }
}
