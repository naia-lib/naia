use std::marker::PhantomData;
use std::vec::IntoIter;
use std::{collections::HashMap, net::SocketAddr};

use naia_shared::{
    Channel, ChannelId, Channels, ComponentId, Components, Message, MessageId, MessageReceivable,
    Messages, ReplicateSafe, Tick,
};

use crate::NaiaClientError;

pub struct Events<E: Copy> {
    connections: Vec<SocketAddr>,
    rejections: Vec<SocketAddr>,
    disconnections: Vec<SocketAddr>,
    ticks: Vec<()>,
    errors: Vec<NaiaClientError>,
    messages: HashMap<ChannelId, HashMap<MessageId, Vec<Box<dyn Message>>>>,
    spawns: Vec<E>,
    despawns: Vec<E>,
    inserts: Vec<(E, ComponentId)>,
    removes: HashMap<ComponentId, Vec<(E, Box<dyn ReplicateSafe>)>>,
    updates: Vec<(Tick, E, ComponentId)>,
    empty: bool,
}

impl<E: Copy> Default for Events<E> {
    fn default() -> Self {
        Events::new()
    }
}

impl<E: Copy> MessageReceivable for Events<E> {}

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

    pub(crate) fn push_message(&mut self, channel_id: &ChannelId, message: Box<dyn Message>) {
        if !self.messages.contains_key(&channel_id) {
            self.messages.insert(*channel_id, HashMap::new());
        }
        let channel_map = self.messages.get_mut(&channel_id).unwrap();

        let message_id: MessageId = Messages::message_id_from_box(&message);
        if !channel_map.contains_key(&message_id) {
            channel_map.insert(message_id, Vec::new());
        }
        let list = channel_map.get_mut(&message_id).unwrap();
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

    pub(crate) fn push_insert(&mut self, entity: E, component_id: ComponentId) {
        self.inserts.push((entity, component_id));
        self.empty = false;
    }

    pub(crate) fn push_remove(&mut self, entity: E, component: Box<dyn ReplicateSafe>) {
        let component_id: ComponentId = Components::box_to_id(&component);
        if !self.removes.contains_key(&component_id) {
            self.removes.insert(component_id, Vec::new());
        }
        let list = self.removes.get_mut(&component_id).unwrap();
        list.push((entity, component));
        self.empty = false;
    }

    pub(crate) fn push_update(&mut self, tick: Tick, entity: E, component_id: ComponentId) {
        self.updates.push((tick, entity, component_id));
        self.empty = false;
    }

    pub(crate) fn clear(&mut self) {
        todo!()
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
impl<E: Copy, C: Channel + 'static, M: Message + 'static> Event<E> for MessageEvent<C, M> {
    type Iter = IntoIter<M>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let channel_id: ChannelId = Channels::type_to_id::<C>();
        if let Some(mut channel_map) = events.messages.remove(&channel_id) {
            let message_id: MessageId = Messages::type_to_id::<M>();
            if let Some(boxed_list) = channel_map.remove(&message_id) {
                let mut output_list: Vec<M> = Vec::new();

                for boxed_message in boxed_list {
                    let message: M = Messages::downcast::<M>(boxed_message)
                        .expect("shouldn't be possible here?");
                    output_list.push(message);
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
    type Iter = IntoIter<(E, ComponentId)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.inserts);
        return IntoIterator::into_iter(list);
    }
}

// Remove Event
pub struct RemoveComponentEvent<C: ReplicateSafe> {
    phantom_c: PhantomData<C>,
}
impl<E: Copy, C: ReplicateSafe> Event<E> for RemoveComponentEvent<C> {
    type Iter = IntoIter<(E, C)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let component_id: ComponentId = Components::type_to_id::<C>();
        if let Some(boxed_list) = events.removes.remove(&component_id) {
            let mut output_list: Vec<(E, C)> = Vec::new();

            for (entity, boxed_component) in boxed_list {
                let component: C =
                    Components::cast::<C>(boxed_component).expect("shouldn't be possible here?");
                output_list.push((entity, component));
            }

            return IntoIterator::into_iter(output_list);
        }

        return IntoIterator::into_iter(Vec::new());
    }
}

// Update Event
pub struct UpdateComponentEvent;
impl<E: Copy> Event<E> for UpdateComponentEvent {
    type Iter = IntoIter<(Tick, E, ComponentId)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.updates);
        return IntoIterator::into_iter(list);
    }
}
