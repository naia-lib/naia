use std::collections::VecDeque;

use naia_serde::{BitWrite, BitWriter, Serde};
use naia_socket_shared::Instant;

use crate::messages::message_channel::MessageChannelSender;
use crate::{
    messages::{message_kinds::MessageKinds, named::Named},
    types::MessageIndex,
    Message, ProtocolIo,
};

use super::message_channel::ChannelSender;

pub struct UnorderedUnreliableSender {
    outgoing_messages: VecDeque<Box<dyn Message>>,
}

impl UnorderedUnreliableSender {
    pub fn new() -> Self {
        Self {
            outgoing_messages: VecDeque::new(),
        }
    }

    fn write_message<S: BitWrite>(
        &self,
        message_kinds: &MessageKinds,
        channel_writer: &ProtocolIo,
        bit_writer: &mut S,
        message: &Box<dyn Message>,
    ) {
        channel_writer.write(message_kinds, bit_writer, message);
    }

    fn warn_overflow(&self, message: &Box<dyn Message>, bits_needed: u16, bits_free: u16) {
        let message_name = message.name();
        panic!(
            "Packet Write Error: Blocking overflow detected! Message of type `{message_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommended to slim down this Message, or send this message over a Reliable channel so it can be Fragmented)"
        )
    }
}

impl ChannelSender<Box<dyn Message>> for UnorderedUnreliableSender {
    fn send_message(&mut self, message: Box<dyn Message>) {
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
        channel_writer: &ProtocolIo,
        bit_writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>> {
        loop {
            if self.outgoing_messages.is_empty() {
                break;
            }

            // Check that we can write the next message
            let message = self.outgoing_messages.front().unwrap();
            let mut counter = bit_writer.counter();
            self.write_message(message_kinds, channel_writer, &mut counter, message);

            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of message being too big
                if !*has_written {
                    self.warn_overflow(message, counter.bits_needed(), bit_writer.bits_free());
                }

                break;
            }

            *has_written = true;

            // write MessageContinue bit
            true.ser(bit_writer);

            // write data
            self.write_message(message_kinds, channel_writer, bit_writer, &message);

            // pop message we've written
            self.outgoing_messages.pop_front();
        }
        None
    }
}
