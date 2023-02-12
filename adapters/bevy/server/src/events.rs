use std::collections::HashMap;

use naia_server::{User, UserKey, shared::{Message, Channel, MessageKind, ChannelKind}, NaiaServerError, Events};

// ConnectEvent
pub struct ConnectEvent(pub UserKey);

// DisconnectEvent
pub struct DisconnectEvent(pub UserKey, pub User);

// ErrorEvent
pub struct ErrorEvent(pub NaiaServerError);

// AuthEvents
pub struct AuthEvents {
    inner: HashMap<MessageKind, Vec<(UserKey, Box<dyn Message>)>>
}
impl From<&mut Events> for AuthEvents {
    fn from(events: &mut Events) -> Self {
        Self {
            inner: events.take_auths()
        }
    }
}

// MessageEvents
pub struct MessageEvents {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, Box<dyn Message>)>>>
}
impl From<&mut Events> for MessageEvents {
    fn from(events: &mut Events) -> Self {
        Self {
            inner: events.take_messages()
        }
    }
}
