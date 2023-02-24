use std::collections::VecDeque;

use naia_serde::BitWriter;
use naia_socket_shared::Instant;

use crate::{
    messages::{
        indexed_message_writer::IndexedMessageWriter,
        message_channel::{ChannelSender, MessageChannelSender},
        message_kinds::MessageKinds,
    },
    types::MessageIndex,
    Message, ProtocolIo,
};

pub struct SequencedUnreliableSender {
    /// Buffer of the next messages to send along with their MessageKind
    outgoing_messages: VecDeque<(MessageIndex, Box<dyn Message>)>,
    /// Next message id to use (not yet used in the buffer)
    next_send_message_index: MessageIndex,
}

impl SequencedUnreliableSender {
    pub fn new() -> Self {
        Self {
            outgoing_messages: VecDeque::new(),
            next_send_message_index: 0,
        }
    }
}

impl ChannelSender<Box<dyn Message>> for SequencedUnreliableSender {
    fn send_message(&mut self, message: Box<dyn Message>) {
        self.outgoing_messages
            .push_back((self.next_send_message_index, message));
        self.next_send_message_index = self.next_send_message_index.wrapping_add(1);
    }

    fn collect_messages(&mut self, _: &Instant, _: &f32) {
        // not necessary for an unreliable channel
    }

    fn has_messages(&self) -> bool {
        !self.outgoing_messages.is_empty()
    }

    fn notify_message_delivered(&mut self, _: &MessageIndex) {
        // not necessary for an unreliable channel
    }
}

impl MessageChannelSender for SequencedUnreliableSender {
    /// Write messages from the buffer into the channel
    /// Include a wrapped message id for sequencing purposes
    fn write_messages(
        &mut self,
        message_kinds: &MessageKinds,
        channel_writer: &ProtocolIo,
        bit_writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>> {
        IndexedMessageWriter::write_messages(
            message_kinds,
            &mut self.outgoing_messages,
            channel_writer,
            bit_writer,
            has_written,
        )
    }
}
