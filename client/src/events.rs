use std::{collections::HashMap, marker::PhantomData, mem, net::SocketAddr, vec::IntoIter};

use naia_shared::{
    Channel, ChannelKind, ComponentKind, DespawnEntityEvent, InsertComponentEvent, Message,
    MessageContainer, MessageKind, RemoveComponentEvent, Replicate, SpawnEntityEvent, Tick,
    UpdateComponentEvent, WorldEvent, WorldEvents,
};

use crate::NaiaClientError;

pub struct Events<E: Copy> {
    connections: Vec<SocketAddr>,
    rejections: Vec<SocketAddr>,
    disconnections: Vec<SocketAddr>,
    client_ticks: Vec<Tick>,
    server_ticks: Vec<Tick>,
    errors: Vec<NaiaClientError>,
    messages: HashMap<ChannelKind, HashMap<MessageKind, Vec<MessageContainer>>>,
    pub world: WorldEvents<E>,
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
            world: WorldEvents::new(),
            empty: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.empty && self.world.is_empty()
    }

    pub fn read<V: Event<E>>(&mut self) -> V::Iter {
        return V::iter(self);
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn take_messages(
        &mut self,
    ) -> HashMap<ChannelKind, HashMap<MessageKind, Vec<MessageContainer>>> {
        mem::take(&mut self.messages)
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

    pub(crate) fn clear(&mut self) {
        self.connections.clear();
        self.rejections.clear();
        self.disconnections.clear();
        self.client_ticks.clear();
        self.server_ticks.clear();
        self.errors.clear();
        self.messages.clear();
        self.empty = true;
    }
}

// Event Trait
pub trait Event<E: Copy> {
    type Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter;
}

// World Events
impl<E: Copy> Event<E> for SpawnEntityEvent {
    type Iter = <Self as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <Self as WorldEvent<E>>::iter(&mut events.world)
    }
}

impl<E: Copy> Event<E> for DespawnEntityEvent {
    type Iter = <Self as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <Self as WorldEvent<E>>::iter(&mut events.world)
    }
}

impl<E: Copy, C: Replicate> Event<E> for InsertComponentEvent<C> {
    type Iter = <Self as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <Self as WorldEvent<E>>::iter(&mut events.world)
    }
}

impl<E: Copy, C: Replicate> Event<E> for UpdateComponentEvent<C> {
    type Iter = <Self as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <Self as WorldEvent<E>>::iter(&mut events.world)
    }
}

impl<E: Copy, C: Replicate> Event<E> for RemoveComponentEvent<C> {
    type Iter = <Self as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <Self as WorldEvent<E>>::iter(&mut events.world)
    }
}

// ConnectEvent
pub struct ConnectEvent;
impl<E: Copy> Event<E> for ConnectEvent {
    type Iter = IntoIter<SocketAddr>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.connections);
        return IntoIterator::into_iter(list);
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
}

// DisconnectEvent
pub struct DisconnectEvent;
impl<E: Copy> Event<E> for DisconnectEvent {
    type Iter = IntoIter<SocketAddr>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.disconnections);
        return IntoIterator::into_iter(list);
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
}

// Server Tick Event
pub struct ServerTickEvent;
impl<E: Copy> Event<E> for ServerTickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.server_ticks);
        return IntoIterator::into_iter(list);
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
        if let Some(mut channel_map) = events.messages.remove(&channel_kind) {
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
}
