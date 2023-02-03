use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::vec::IntoIter;

use naia_shared::Channel;

use super::user::{User, UserKey};
use crate::NaiaServerError;

pub struct Events {
    connections: Vec<(UserKey)>,
    disconnections: Vec<(UserKey, User)>,
    ticks: Vec<()>,
    errors: Vec<(NaiaServerError)>,
    auths: HashMap<TypeId, Vec<(UserKey, Box<dyn Any>)>>,
    messages: HashMap<TypeId, HashMap<TypeId, Vec<(UserKey, Box<dyn Any>)>>>,
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

    // Crate-public

    pub(crate) fn push_connection(&mut self, user_key: &UserKey) {
        self.connections.push((*user_key));
        self.empty = false;
    }

    pub(crate) fn push_disconnection(&mut self, user_key: &UserKey, user: User) {
        self.disconnections.push((*user_key, user));
        self.empty = false;
    }

    pub(crate) fn push_auth<M: Any>(&mut self, user_key: &UserKey, auth_message: M) {
        let type_id: TypeId = TypeId::of::<M>();
        if !self.auths.contains_key(&type_id) {
            self.auths.insert(type_id, Vec::new());
        }
        let list = self.auths.get_mut(&type_id).unwrap();
        list.push((*user_key, Box::new(auth_message)));
        self.empty = false;
    }

    pub(crate) fn push_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: M) {
        let channel_type: TypeId = TypeId::of::<C>();
        if !self.messages.contains_key(&channel_type) {
            self.messages.insert(channel_type, HashMap::new());
        }
        let channel_map = self.messages.get_mut(&channel_type).unwrap();

        let message_type: TypeId = TypeId::of::<M>();
        if !channel_map.contains_key(&message_type) {
            channel_map.insert(message_type, Vec::new());
        }
        let list = channel_map.get_mut(&message_type).unwrap();
        list.push((*user_key, Box::new(message)));
        self.empty = false;
    }

    pub(crate) fn push_tick(&mut self) {
        self.ticks.push(());
        self.empty = false;
    }

    pub(crate) fn push_error(&mut self, error: NaiaServerError) {
        self.errors.push((error));
        self.empty = false;
    }
}

// Event Trait
pub trait Event {
    type Iter;

    fn iter(events: &mut Events) -> Self::Iter;
}

// ConnectEvent
pub struct ConnectionEvent(pub UserKey);
impl Event for ConnectionEvent {
    type Iter = IntoIter<(UserKey)>;

    fn iter(events: &mut Events) -> Self::Iter {
        let list = std::mem::take(&mut events.connections);
        return IntoIterator::into_iter(list);
    }
}

// DisconnectEvent
pub struct DisconnectionEvent(pub UserKey, pub User);
impl Event for DisconnectionEvent {
    type Iter = IntoIter<(UserKey, User)>;

    fn iter(events: &mut Events) -> Self::Iter {
        let list = std::mem::take(&mut events.disconnections);
        return IntoIterator::into_iter(list);
    }
}

// Tick Event
pub struct TickEvent;
impl Event for TickEvent {
    type Iter = IntoIter<()>;

    fn iter(events: &mut Events) -> Self::Iter {
        let list = std::mem::take(&mut events.ticks);
        return IntoIterator::into_iter(list);
    }
}

// Error Event
pub struct ErrorEvent(pub NaiaServerError);
impl Event for ErrorEvent {
    type Iter = IntoIter<(NaiaServerError)>;

    fn iter(events: &mut Events) -> Self::Iter {
        let list = std::mem::take(&mut events.errors);
        return IntoIterator::into_iter(list);
    }
}

// Auth Event
pub struct AuthorizationEvent<M>(pub UserKey, pub M);
impl<M: 'static> Event for AuthorizationEvent<M> {
    type Iter = IntoIter<(UserKey, M)>;

    fn iter(events: &mut Events) -> Self::Iter {
        let type_id: TypeId = TypeId::of::<M>();
        if let Some(boxed_list) = events.auths.remove(&type_id) {
            let mut output_list: Vec<(UserKey, M)> = Vec::new();

            for (user_key, boxed_auth) in boxed_list {
                let message: M = *boxed_auth
                    .downcast::<M>()
                    .expect("shouldn't be possible here?");
                output_list.push((user_key, message));
            }

            return IntoIterator::into_iter(output_list);
        } else {
            return IntoIterator::into_iter(Vec::new());
        }
    }
}

// Message Event
pub struct MessageEvent<C: ChannelType, M>(pub UserKey, pub C, pub M);
impl<C: ChannelType + 'static, M: 'static> Event for MessageEvent<C, M> {
    type Iter = IntoIter<(UserKey, C, M)>;

    fn iter(events: &mut Events) -> Self::Iter {
        let channel_type: TypeId = TypeId::of::<C>();
        if let Some(mut channel_map) = events.messages.remove(&channel_type) {
            let message_type: TypeId = TypeId::of::<M>();
            if let Some(boxed_list) = channel_map.remove(&message_type) {
                let mut output_list: Vec<(UserKey, C, M)> = Vec::new();

                for (user_key, boxed_auth) in boxed_list {
                    let message: M = *boxed_auth
                        .downcast::<M>()
                        .expect("shouldn't be possible here?");
                    output_list.push((user_key, C::new(), message));
                }

                return IntoIterator::into_iter(output_list);
            }
        }
        return IntoIterator::into_iter(Vec::new());
    }
}
