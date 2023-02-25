use std::collections::VecDeque;

use naia_serde::{BitWrite, BitWriter, Serde};
use naia_socket_shared::Instant;

use crate::{
    messages::{
        message_channel::MessageChannelSender, message_container::MessageContainer,
        message_kinds::MessageKinds,
    },
    types::MessageIndex,
    NetEntityHandleConverter,
};

use super::message_channel::ChannelSender;

pub struct UnorderedUnreliableSender {
    outgoing_messages: VecDeque<MessageContainer>,
}

impl UnorderedUnreliableSender {
    pub fn new() -> Self {
        Self {
            outgoing_messages: VecDeque::new(),
        }
    }

    fn write_message(
        &self,
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut dyn BitWrite,
        message: &MessageContainer,
    ) {
        message.write(message_kinds, writer, converter);
    }

    fn warn_overflow(&self, message: &MessageContainer, bits_needed: u32, bits_free: u32) {
        let message_name = message.name();
        panic!(
            "Packet Write Error: Blocking overflow detected! Message of type `{message_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommended to slim down this Message, or send this message over a Reliable channel so it can be Fragmented)"
        )
    }
}

impl ChannelSender<MessageContainer> for UnorderedUnreliableSender {
    fn send_message(&mut self, message: MessageContainer) {
        self.outgoing_messages.push_back(message);
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

impl MessageChannelSender for UnorderedUnreliableSender {
    fn write_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>> {
        loop {
            if self.outgoing_messages.is_empty() {
                break;
            }

            // Check that we can write the next message
            let message = self.outgoing_messages.front().unwrap();
            let mut counter = writer.counter();
            self.write_message(message_kinds, converter, &mut counter, message);

            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of message being too big
                if !*has_written {
                    self.warn_overflow(message, counter.bits_needed(), writer.bits_free());
                }

                break;
            }

            *has_written = true;

            // write MessageContinue bit
            true.ser(writer);

            // write data
            self.write_message(message_kinds, converter, writer, &message);

            // pop message we've written
            self.outgoing_messages.pop_front();
        }
        None
    }
}
