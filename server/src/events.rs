use std::{any::Any, collections::HashMap, marker::PhantomData, mem, vec::IntoIter};

use naia_shared::{Channel, ChannelKind, Message, MessageKind, Tick};

use super::user::{User, UserKey};
use crate::NaiaServerError;

pub struct Events {
    connections: Vec<UserKey>,
    disconnections: Vec<(UserKey, User)>,
    ticks: Vec<Tick>,
    errors: Vec<NaiaServerError>,
    auths: HashMap<MessageKind, Vec<(UserKey, Box<dyn Message>)>>,
    messages: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, Box<dyn Message>)>>>,
    empty: bool,
}

impl Default for Events {
    fn default() -> Self {
        Events::new()
    }
}

impl Events {
    pub(crate) fn new() -> Events {
        Self {
            connections: Vec::new(),
            disconnections: Vec::new(),
            ticks: Vec::new(),
            errors: Vec::new(),
            auths: HashMap::new(),
            messages: HashMap::new(),
            empty: true,
        }
    }

    // Public

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn read<E: Event>(&mut self) -> E::Iter {
        return E::iter(self);
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn take_messages(
        &mut self,
    ) -> HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, Box<dyn Message>)>>> {
        mem::take(&mut self.messages)
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn take_auths(&mut self) -> HashMap<MessageKind, Vec<(UserKey, Box<dyn Message>)>> {
        mem::take(&mut self.auths)
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

    pub(crate) fn push_auth(&mut self, user_key: &UserKey, auth_message: Box<dyn Message>) {
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
        message: Box<dyn Message>,
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
}

// Event Trait
pub trait Event {
    type Iter;

    fn iter(events: &mut Events) -> Self::Iter;
}

// ConnectEvent
pub struct ConnectEvent;
impl Event for ConnectEvent {
    type Iter = IntoIter<UserKey>;

    fn iter(events: &mut Events) -> Self::Iter {
        let list = std::mem::take(&mut events.connections);
        return IntoIterator::into_iter(list);
    }
}

// DisconnectEvent
pub struct DisconnectEvent;
impl Event for DisconnectEvent {
    type Iter = IntoIter<(UserKey, User)>;

    fn iter(events: &mut Events) -> Self::Iter {
        let list = std::mem::take(&mut events.disconnections);
        return IntoIterator::into_iter(list);
    }
}

// Tick Event
pub struct TickEvent;
impl Event for TickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut Events) -> Self::Iter {
        let list = std::mem::take(&mut events.ticks);
        return IntoIterator::into_iter(list);
    }
}

// Error Event
pub struct ErrorEvent;
impl Event for ErrorEvent {
    type Iter = IntoIter<NaiaServerError>;

    fn iter(events: &mut Events) -> Self::Iter {
        let list = std::mem::take(&mut events.errors);
        return IntoIterator::into_iter(list);
    }
}

// Auth Event
pub struct AuthEvent<M: Message> {
    phantom_m: PhantomData<M>,
}
impl<M: Message> Event for AuthEvent<M> {
    type Iter = IntoIter<(UserKey, M)>;

    fn iter(events: &mut Events) -> Self::Iter {
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
impl<C: Channel, M: Message> Event for MessageEvent<C, M> {
    type Iter = IntoIter<(UserKey, M)>;

    fn iter(events: &mut Events) -> Self::Iter {
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
