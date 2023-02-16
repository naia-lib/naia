use std::{any::Any, collections::HashMap};

use naia_bevy_shared::{Channel, ChannelKind, Message, MessageKind, Tick};
use naia_server::{Events, NaiaServerError, User, UserKey};

// ConnectEvent
pub struct ConnectEvent(pub UserKey);

// DisconnectEvent
pub struct DisconnectEvent(pub UserKey, pub User);

// ErrorEvent
pub struct ErrorEvent(pub NaiaServerError);

// TickEvent
pub struct TickEvent(pub Tick);

// MessageEvents
pub struct MessageEvents {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, Box<dyn Message>)>>>,
}

impl From<&mut Events> for MessageEvents {
    fn from(events: &mut Events) -> Self {
        Self {
            inner: events.take_messages(),
        }
    }
}

impl MessageEvents {
    pub fn read<C: Channel, M: Message>(&self) -> Vec<(UserKey, M)> {
        let mut output = Vec::new();

        let channel_kind = ChannelKind::of::<C>();
        if let Some(message_map) = self.inner.get(&channel_kind) {
            let message_kind = MessageKind::of::<M>();
            if let Some(messages) = message_map.get(&message_kind) {
                for (user_key, boxed_message) in messages {
                    let boxed_any = boxed_message.clone_box().to_boxed_any();
                    let message: M = Box::<dyn Any + 'static>::downcast::<M>(boxed_any)
                        .ok()
                        .map(|boxed_m| *boxed_m)
                        .unwrap();
                    output.push((*user_key, message));
                }
            }
        }

        output
    }
}

// AuthEvents
pub struct AuthEvents {
    inner: HashMap<MessageKind, Vec<(UserKey, Box<dyn Message>)>>,
}

impl From<&mut Events> for AuthEvents {
    fn from(events: &mut Events) -> Self {
        Self {
            inner: events.take_auths(),
        }
    }
}

impl AuthEvents {
    pub fn read<M: Message>(&self) -> Vec<(UserKey, M)> {
        let mut output = Vec::new();

        let message_kind = MessageKind::of::<M>();

        if let Some(messages) = self.inner.get(&message_kind) {
            for (user_key, boxed_message) in messages {
                let boxed_any = boxed_message.clone_box().to_boxed_any();
                let message: M = Box::<dyn Any + 'static>::downcast::<M>(boxed_any)
                    .ok()
                    .map(|boxed_m| *boxed_m)
                    .unwrap();
                output.push((*user_key, message));
            }
        }

        output
    }
}
