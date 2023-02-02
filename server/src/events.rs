use std::marker::PhantomData;
use naia_shared::{ChannelIndex, Protocolize};
use crate::NaiaServerError;

use super::user::{User, UserKey};

pub struct Events {

}

impl Events {
    pub(crate) fn new() -> Events {
        Self {

        }
    }

    // Public

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    pub fn read<E>(&mut self) -> EventIterator<E> {
        todo!()
    }

    // Crate-public

    pub(crate) fn push_connection(&mut self, user_key: &UserKey) {
        todo!()
    }

    pub(crate) fn push_disconnection(&mut self, user_key: &UserKey, user: User) {
        todo!()
    }

    pub(crate) fn push_auth<M>(&mut self, user_key: &UserKey, auth_message: M) {
        todo!()
    }

    pub(crate) fn push_message<C, M>(&mut self, user_key: &UserKey, channel: C, message: M) {
        todo!()
    }

    pub(crate) fn push_tick(&mut self) {
        todo!()
    }

    pub(crate) fn push_error(&mut self, error: NaiaServerError) {
        todo!()
    }
}

impl Default for Events {
    fn default() -> Self {
        Events::new()
    }
}

// EventIterator
pub struct EventIterator<E> {
    phantom_e: PhantomData<E>
}

impl<E> EventIterator<E> {
    pub fn new() -> Self {
        Self {
            phantom_e: PhantomData::<E>,
        }
    }
}

impl<E> Iterator for EventIterator<E> {
    type Item = E;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

// Specific Events
pub struct AuthorizationEvent<M>(pub UserKey, pub M);
pub struct ConnectionEvent(pub UserKey);
pub struct DisconnectionEvent(pub UserKey, pub User);
pub struct MessageEvent<C, M>(pub UserKey, pub C, pub M);
pub struct TickEvent;
pub struct ErrorEvent(pub NaiaServerError);
