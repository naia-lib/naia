use std::collections::VecDeque;

use naia_serde::{BitCounter, BitWrite, BitWriter};
use naia_socket_shared::Instant;

use crate::{constants::MTU_SIZE_BITS, types::MessageId};

use super::{
    message_channel::{ChannelSender, ChannelWriter},
    message_list_header::write,
};

pub struct UnorderedUnreliableSender<P: Send> {
    outgoing_messages: VecDeque<P>,
}

impl<P: Send> UnorderedUnreliableSender<P> {
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
        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = bit_writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                write(bit_writer, 0);
                return None;
            }

            let mut counter = BitCounter::new();

            //TODO: message_count is inaccurate here and may be different than final, does
            // this matter?
            write(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                write(bit_writer, 0);
                return None;
            }

            // Find how many messages will fit into the packet
            let mut index = 0;
            loop {
                if index >= self.outgoing_messages.len() {
                    break;
                }

                let message = self.outgoing_messages.get(index).unwrap();
                self.write_message(channel_writer, &mut counter, message);
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }

                index += 1;
            }
        }

        // Write header
        write(bit_writer, message_count);

        // Messages
        {
            for _ in 0..message_count {
                // Pop and write message
                let message = self.outgoing_messages.pop_front().unwrap();
                self.write_message(channel_writer, bit_writer, &message);
            }
            None
        }
    }

    fn notify_message_delivered(&mut self, _: &MessageId) {
        // not necessary for an unreliable channel
    }
}
