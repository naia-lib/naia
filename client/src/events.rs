use std::{collections::HashMap, marker::PhantomData, net::SocketAddr, vec::IntoIter};

use naia_shared::{Channel, ChannelKind, ComponentKind, Message, MessageKind, Replicate, Tick};

use crate::NaiaClientError;

pub struct Events<E: Copy> {
    connections: Vec<SocketAddr>,
    rejections: Vec<SocketAddr>,
    disconnections: Vec<SocketAddr>,
    ticks: Vec<()>,
    errors: Vec<NaiaClientError>,
    messages: HashMap<ChannelKind, HashMap<MessageKind, Vec<Box<dyn Message>>>>,
    spawns: Vec<E>,
    despawns: Vec<E>,
    inserts: Vec<(E, ComponentKind)>,
    removes: HashMap<ComponentKind, Vec<(E, Box<dyn Replicate>)>>,
    updates: Vec<(Tick, E, ComponentKind)>,
    empty: bool,
}

impl<E: Copy> Default for Events<E> {
    fn default() -> Self {
        Events::new()
    }
}

impl<E: Copy> Events<E> {
    pub(crate) fn new() -> Events<E> {
        Self {
            connections: Vec::new(),
            rejections: Vec::new(),
            disconnections: Vec::new(),
            ticks: Vec::new(),
            errors: Vec::new(),
            messages: HashMap::new(),
            spawns: Vec::new(),
            despawns: Vec::new(),
            inserts: Vec::new(),
            removes: HashMap::new(),
            updates: Vec::new(),
            empty: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn read<V: Event<E>>(&mut self) -> V::Iter {
        return V::iter(self);
    }

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

    pub(crate) fn push_message(&mut self, channel_kind: &ChannelKind, message: Box<dyn Message>) {
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

    pub(crate) fn push_tick(&mut self) {
        self.ticks.push(());
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

    pub(crate) fn push_insert(&mut self, entity: E, component_kind: ComponentKind) {
        self.inserts.push((entity, component_kind));
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

    pub(crate) fn push_update(&mut self, tick: Tick, entity: E, component_kind: ComponentKind) {
        self.updates.push((tick, entity, component_kind));
        self.empty = false;
    }

    pub(crate) fn clear(&mut self) {
        self.connections.clear();
        self.rejections.clear();
        self.disconnections.clear();
        self.ticks.clear();
        self.errors.clear();
        self.messages.clear();
        self.spawns.clear();
        self.despawns.clear();
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
}

// ConnectEvent
pub struct ConnectionEvent;
impl<E: Copy> Event<E> for ConnectionEvent {
    type Iter = IntoIter<SocketAddr>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.connections);
        return IntoIterator::into_iter(list);
    }
}

// RejectEvent
pub struct RejectionEvent;
impl<E: Copy> Event<E> for RejectionEvent {
    type Iter = IntoIter<SocketAddr>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.rejections);
        return IntoIterator::into_iter(list);
    }
}

// DisconnectEvent
pub struct DisconnectionEvent;
impl<E: Copy> Event<E> for DisconnectionEvent {
    type Iter = IntoIter<SocketAddr>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.disconnections);
        return IntoIterator::into_iter(list);
    }
}

// Tick Event
pub struct TickEvent;
impl<E: Copy> Event<E> for TickEvent {
    type Iter = IntoIter<()>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.ticks);
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

// Spawn Event
pub struct SpawnEntityEvent;
impl<E: Copy> Event<E> for SpawnEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.spawns);
        return IntoIterator::into_iter(list);
    }
}

// Despawn Event
pub struct DespawnEntityEvent;
impl<E: Copy> Event<E> for DespawnEntityEvent {
    type Iter = IntoIter<E>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.despawns);
        return IntoIterator::into_iter(list);
    }
}

// Insert Event
pub struct InsertComponentEvent;
impl<E: Copy> Event<E> for InsertComponentEvent {
    type Iter = IntoIter<(E, ComponentKind)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.inserts);
        return IntoIterator::into_iter(list);
    }
}

// Remove Event
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
}

// Update Event
pub struct UpdateComponentEvent;
impl<E: Copy> Event<E> for UpdateComponentEvent {
    type Iter = IntoIter<(Tick, E, ComponentKind)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.updates);
        return IntoIterator::into_iter(list);
    }
}
