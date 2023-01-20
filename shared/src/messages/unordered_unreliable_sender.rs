use std::collections::VecDeque;

use naia_serde::{BitWrite, BitWriter, Serde};
use naia_socket_shared::Instant;

use crate::types::MessageId;

use super::message_channel::{ChannelSender, ChannelWriter};

pub struct UnorderedUnreliableSender<P: Send> {
    outgoing_messages: VecDeque<P>,
}

impl<P: Send> UnorderedUnreliableSender<P> {
    pub fn new() -> Self {
        Self {
            outgoing_messages: VecDeque::new(),
        }
    }

    fn write_outgoing_message<S: BitWrite>(
        &self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut S,
        message: &P,
    ) {
        channel_writer.write(bit_writer, message);
    }
}

impl<P: Send + Sync> ChannelSender<P> for UnorderedUnreliableSender<P> {
    fn send_message(&mut self, message: P) {
        self.outgoing_messages.push_back(message);
    }

    fn collect_messages(&mut self, _: &Instant, _: &f32) {
        // not necessary for an unreliable channel
    }

    fn has_messages(&self) -> bool {
        !self.outgoing_messages.is_empty()
    }

    fn write_messages(
        &mut self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut BitWriter,
    ) -> Option<Vec<MessageId>> {

        loop {

            if self.outgoing_messages.is_empty() {
                break;
            }

            // Check that we can write the next message
            let message = self.outgoing_messages.front().unwrap();
            let mut counter = bit_writer.counter();
            self.write_outgoing_message(
                channel_writer,
                &mut counter,
                &message,
            );

            // if we can, start writing
            if !counter.is_valid() { break; }

            // write MessageContinue bit
            true.ser(bit_writer);

            // write data
            self.write_outgoing_message(channel_writer, bit_writer, &message);

            // pop message we've written
            self.outgoing_messages.pop_front();
        }
        None
    }

    fn notify_message_delivered(&mut self, _: &MessageId) {
        // not necessary for an unreliable channel
    }
}
