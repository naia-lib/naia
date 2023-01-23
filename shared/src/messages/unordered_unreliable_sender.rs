use log::warn;
use std::collections::VecDeque;

use naia_serde::{BitWrite, BitWriter, Serde};
use naia_socket_shared::Instant;

use crate::{types::MessageId, Protocolize};

use super::message_channel::{ChannelSender, ChannelWriter};

pub struct UnorderedUnreliableSender<P: Protocolize> {
    outgoing_messages: VecDeque<P>,
}

impl<P: Protocolize> UnorderedUnreliableSender<P> {
    pub fn new() -> Self {
        Self {
            outgoing_messages: VecDeque::new(),
        }
    }

    fn write_message<S: BitWrite>(
        &self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut S,
        message: &P,
    ) {
        channel_writer.write(bit_writer, message);
    }

    fn warn_overflow(&self, message: &P, bits_needed: u16, bits_free: u16) {
        let message_name = message.name();
        warn!(
            "Packet Write Error: Blocking overflow detected! Message of type `{message_name}` requires {bits_needed} bits, but packet only has {bits_free} bits available! Recommended to slim down this Message, or send this message over a Reliable channel so it can be Fragmented)"
        )
    }
}

impl<P: Protocolize + Send + Sync> ChannelSender<P> for UnorderedUnreliableSender<P> {
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
        has_written: &mut bool,
    ) -> Option<Vec<MessageId>> {
        loop {
            if self.outgoing_messages.is_empty() {
                break;
            }

            // Check that we can write the next message
            let message = self.outgoing_messages.front().unwrap();
            let mut counter = bit_writer.counter();
            self.write_message(channel_writer, &mut counter, message);

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
            self.write_message(channel_writer, bit_writer, &message);

            // pop message we've written
            self.outgoing_messages.pop_front();
        }
        None
    }

    fn notify_message_delivered(&mut self, _: &MessageId) {
        // not necessary for an unreliable channel
    }
}
