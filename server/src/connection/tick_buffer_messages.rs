use std::collections::HashMap;

use naia_shared::{Channel, ChannelKind, Message, MessageContainer, MessageKind};

use crate::{events, UserKey};

pub struct TickBufferMessages {
    messages: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>>,
    empty: bool,
}

impl TickBufferMessages {
    pub fn new() -> Self {
        Self {
            messages: HashMap::new(),
            empty: true,
        }
    }

    pub(crate) fn push_message(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        message: MessageContainer,
    ) {
        events::push_message_impl(&mut self.messages, user_key, channel_kind, message);
        self.empty = false;
    }

    pub fn read<C: Channel, M: Message>(&mut self) -> Vec<(UserKey, M)> {
        return events::read_channel_messages::<C, M>(&mut self.messages);
    }
}
