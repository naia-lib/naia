use std::{any::Any, collections::HashMap, marker::PhantomData, mem, vec::IntoIter};

use naia_shared::{
    Channel, ChannelKind, ComponentKind, Message, MessageContainer, MessageKind, Replicate, Tick,
};

use super::user::{User, UserKey};

use crate::NaiaServerError;

pub struct Events<E: Copy> {
    connections: Vec<UserKey>,
    disconnections: Vec<(UserKey, User)>,
    ticks: Vec<Tick>,
    errors: Vec<NaiaServerError>,
    auths: HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>,
    messages: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>>,
    spawns: Vec<E>,
    despawns: Vec<E>,
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
            disconnections: Vec::new(),
            ticks: Vec::new(),
            errors: Vec::new(),
            auths: HashMap::new(),
            messages: HashMap::new(),
            spawns: Vec::new(),
            despawns: Vec::new(),
            inserts: HashMap::new(),
            removes: HashMap::new(),
            updates: HashMap::new(),
            empty: true,
        }
    }

    // Public

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn read<V: Event<E>>(&mut self) -> V::Iter {
        return V::iter(self);
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn take_messages(
        &mut self,
    ) -> HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>> {
        mem::take(&mut self.messages)
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn take_auths(&mut self) -> HashMap<MessageKind, Vec<(UserKey, MessageContainer)>> {
        mem::take(&mut self.auths)
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_inserts(&self) -> bool {
        !self.inserts.is_empty()
    }
    pub fn take_inserts(&mut self) -> HashMap<ComponentKind, Vec<E>> {
        mem::take(&mut self.inserts)
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_updates(&self) -> bool {
        !self.updates.is_empty()
    }
    pub fn take_updates(&mut self) -> HashMap<ComponentKind, Vec<(Tick, E)>> {
        mem::take(&mut self.updates)
    }

    // These method are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_removes(&self) -> bool {
        !self.removes.is_empty()
    }
    pub fn take_removes(&mut self) -> HashMap<ComponentKind, Vec<(E, Box<dyn Replicate>)>> {
        mem::take(&mut self.removes)
    }

    // Crate-public

    pub(crate) fn push_connection(&mut self, user_key: &UserKey) {
        self.connections.push(*user_key);
        self.empty = false;
    }

    pub(crate) fn push_disconnection(&mut self, user_key: &UserKey, user: User) {
        self.disconnections.push((*user_key, user));
        self.empty = false;
    }

    pub(crate) fn push_auth(&mut self, user_key: &UserKey, auth_message: MessageContainer) {
        let auth_message_type_id = auth_message.kind();
        if !self.auths.contains_key(&auth_message_type_id) {
            self.auths.insert(auth_message_type_id, Vec::new());
        }
        let list = self.auths.get_mut(&auth_message_type_id).unwrap();
        list.push((*user_key, auth_message));
        self.empty = false;
    }

    pub(crate) fn push_message(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        message: MessageContainer,
    ) {
        if !self.messages.contains_key(&channel_kind) {
            self.messages.insert(*channel_kind, HashMap::new());
        }
        let channel_map = self.messages.get_mut(&channel_kind).unwrap();
        let message_type_id = message.kind();
        if !channel_map.contains_key(&message_type_id) {
            channel_map.insert(message_type_id, Vec::new());
        }
        let list = channel_map.get_mut(&message_type_id).unwrap();
        list.push((*user_key, message));
        self.empty = false;
    }

    pub(crate) fn push_tick(&mut self, tick: Tick) {
        self.ticks.push(tick);
        self.empty = false;
    }

    pub(crate) fn push_error(&mut self, error: NaiaServerError) {
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
        if !self.inserts.contains_key(&component_kind) {
            self.inserts.insert(component_kind, Vec::new());
        }
        let list = self.inserts.get_mut(&component_kind).unwrap();
        list.push(entity);
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
        if !self.updates.contains_key(&component_kind) {
            self.updates.insert(component_kind, Vec::new());
        }
        let list = self.updates.get_mut(&component_kind).unwrap();
        list.push((tick, entity));
        self.empty = false;
    }
}

// Event Trait
pub trait Event<E: Copy> {
    type Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter;
}

// ConnectEvent
pub struct ConnectEvent;
impl<E: Copy> Event<E> for ConnectEvent {
    type Iter = IntoIter<UserKey>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.connections);
        return IntoIterator::into_iter(list);
    }
}

// DisconnectEvent
pub struct DisconnectEvent;
impl<E: Copy> Event<E> for DisconnectEvent {
    type Iter = IntoIter<(UserKey, User)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.disconnections);
        return IntoIterator::into_iter(list);
    }
}

// Tick Event
pub struct TickEvent;
impl<E: Copy> Event<E> for TickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.ticks);
        return IntoIterator::into_iter(list);
    }
}

// Error Event
pub struct ErrorEvent;
impl<E: Copy> Event<E> for ErrorEvent {
    type Iter = IntoIter<NaiaServerError>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let list = std::mem::take(&mut events.errors);
        return IntoIterator::into_iter(list);
    }
}

// Auth Event
pub struct AuthEvent<M: Message> {
    phantom_m: PhantomData<M>,
}
impl<E: Copy, M: Message> Event<E> for AuthEvent<M> {
    type Iter = IntoIter<(UserKey, M)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let message_kind: MessageKind = MessageKind::of::<M>();
        if let Some(boxed_list) = events.auths.remove(&message_kind) {
            let mut output_list: Vec<(UserKey, M)> = Vec::new();

            for (user_key, boxed_auth) in boxed_list {
                let message: M = Box::<dyn Any + 'static>::downcast::<M>(boxed_auth.to_boxed_any())
                    .ok()
                    .map(|boxed_m| *boxed_m)
                    .unwrap();
                output_list.push((user_key, message));
            }

            return IntoIterator::into_iter(output_list);
        } else {
            return IntoIterator::into_iter(Vec::new());
        }
    }
}

// Message Event
pub struct MessageEvent<C: Channel, M: Message> {
    phantom_c: PhantomData<C>,
    phantom_m: PhantomData<M>,
}
impl<E: Copy, C: Channel, M: Message> Event<E> for MessageEvent<C, M> {
    type Iter = IntoIter<(UserKey, M)>;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        let channel_kind: ChannelKind = ChannelKind::of::<C>();
        if let Some(mut channel_map) = events.messages.remove(&channel_kind) {
            let message_kind: MessageKind = MessageKind::of::<M>();
            if let Some(boxed_list) = channel_map.remove(&message_kind) {
                let mut output_list: Vec<(UserKey, M)> = Vec::new();

                for (user_key, boxed_message) in boxed_list {
                    let message: M =
                        Box::<dyn Any + 'static>::downcast::<M>(boxed_message.to_boxed_any())
                            .ok()
                            .map(|boxed_m| *boxed_m)
                            .unwrap();
                    output_list.push((user_key, message));
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
}

// Update Event
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
