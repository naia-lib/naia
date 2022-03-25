use std::collections::VecDeque;

use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde};

use crate::{
    constants::MTU_SIZE_BITS,
    protocol::{
        entity_property::NetEntityHandleConverter, manifest::Manifest, protocolize::Protocolize,
    },
    types::MessageId,
};

use super::{
    channel_config::ChannelIndex,
    message_channel::MessageChannel,
    message_list_header::{read, write},
};

pub struct UnorderedUnreliableChannel<P: Protocolize, C: ChannelIndex> {
    channel_index: C,
    outgoing_messages: VecDeque<P>,
    incoming_messages: VecDeque<P>,
}

impl<P: Protocolize, C: ChannelIndex> UnorderedUnreliableChannel<P, C> {
    pub fn new(channel_index: C) -> Self {
        Self {
            channel_index: channel_index.clone(),
            outgoing_messages: VecDeque::new(),
            incoming_messages: VecDeque::new(),
        }
    }

    pub fn write_message<S: BitWrite>(
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

    pub fn read_message(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    ) -> P {
        // read message kind
        let component_kind: P::Kind = P::Kind::de(reader).unwrap();

        // read payload
        let new_message = manifest.create_replica(component_kind, reader, converter);

        return new_message;
    }

    fn recv_message(&mut self, message: P) {
        self.outgoing_messages.push_back(message);
    }
}

impl<P: Protocolize, C: ChannelIndex> MessageChannel<P, C> for UnorderedUnreliableChannel<P, C> {
    fn send_message(&mut self, message: P) {
        self.incoming_messages.push_back(message);
    }

    fn collect_outgoing_messages(&mut self, _: &f32) {
        // not necessary for an unreliable channel
    }

    fn collect_incoming_messages(&mut self, incoming_messages: &mut Vec<(C, P)>) {
        while let Some(message) = self.incoming_messages.pop_front() {
            incoming_messages.push((self.channel_index.clone(), message));
        }
    }

    fn notify_message_delivered(&mut self, _: &MessageId) {
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

    fn read_messages(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    ) {
        let message_count = read(reader);
        for _x in 0..message_count {
            let message = self.read_message(reader, manifest, converter);
            self.recv_message(message);
        }
    }
}
