use std::collections::HashMap;

use naia_shared::{Channel, ChannelKind, Message, MessageContainer, MessageKind};

use crate::{events::world_events, UserKey};

/// Batch of tick-buffered client messages drained for a single server tick.
///
/// Obtained from [`Server::receive_tick_buffer_messages`](crate::Server::receive_tick_buffer_messages).
/// Iterate by calling [`read::<C, M>()`](TickBufferMessages::read) for each channel/message type pair.
pub struct TickBufferMessages {
    messages: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>>,
    empty: bool,
}

impl Default for TickBufferMessages {
    fn default() -> Self {
        Self::new()
    }
}

impl TickBufferMessages {
    /// Creates an empty `TickBufferMessages` container.
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
        world_events::push_message_impl(&mut self.messages, user_key, channel_kind, message);
        self.empty = false;
    }

    /// Drains and returns all `(UserKey, M)` pairs buffered for channel `C` and message type `M`.
    pub fn read<C: Channel, M: Message>(&mut self) -> Vec<(UserKey, M)> {
        world_events::read_channel_messages::<C, M>(&mut self.messages)
    }
}
