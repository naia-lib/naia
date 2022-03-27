use std::collections::VecDeque;

use naia_serde::{BitCounter, BitWrite, BitWriter, Serde};

use crate::{
    constants::MTU_SIZE_BITS,
    protocol::{entity_property::NetEntityHandleConverter, protocolize::Protocolize},
    types::MessageId,
};

use super::{message_channel::ChannelSender, message_list_header::write};

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
        writer: &mut S,
        converter: &dyn NetEntityHandleConverter,
        message: &P,
    ) {
        // write message kind
        message.dyn_ref().kind().ser(writer);

        // write payload
        message.write(writer, converter);
    }
}

impl<P: Protocolize> ChannelSender<P> for UnorderedUnreliableSender<P> {
    fn send_message(&mut self, message: P) {
        self.outgoing_messages.push_back(message);
    }

    fn collect_outgoing_messages(&mut self, _: &f32) {
        // not necessary for an unreliable channel
    }

    fn has_outgoing_messages(&self) -> bool {
        return self.outgoing_messages.len() != 0;
    }

    fn write_messages(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
    ) -> Option<Vec<MessageId>> {
        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                write(writer, 0);
                return None;
            }

            let mut counter = BitCounter::new();

            //TODO: message_count is inaccurate here and may be different than final, does
            // this matter?
            write(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                write(writer, 0);
                return None;
            }

            // Find how many messages will fit into the packet
            let mut index = 0;
            loop {
                if index >= self.outgoing_messages.len() {
                    break;
                }

                let message = self.outgoing_messages.get(index).unwrap();
                self.write_message(&mut counter, converter, message);
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }

                index += 1;
            }
        }

        // Write header
        write(writer, message_count);

        // Messages
        {
            for _ in 0..message_count {
                // Pop and write message
                let message = self.outgoing_messages.pop_front().unwrap();
                self.write_message(writer, converter, &message);
            }
            return None;
        }
    }

    fn notify_message_delivered(&mut self, _: &MessageId) {
        // not necessary for an unreliable channel
    }
}
