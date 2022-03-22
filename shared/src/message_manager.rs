use std::{
    collections::{HashMap, VecDeque},
    vec::Vec,
};

use crate::{
    constants::MTU_SIZE_BITS, read_list_header, write_list_header, ChannelConfig, ChannelIndex,
    NetEntityHandleConverter, PacketIndex,
};
use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde};

use super::{
    manifest::Manifest, packet_notifiable::PacketNotifiable, protocolize::Protocolize,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct MessageManager<P: Protocolize, C: ChannelIndex> {
    channel_config: ChannelConfig<C>,
    queued_outgoing_messages: VecDeque<(C, P)>,
    queued_incoming_messages: VecDeque<(C, P)>,
    sent_guaranteed_messages: HashMap<PacketIndex, Vec<(C, P)>>,
}

impl<P: Protocolize, C: ChannelIndex> MessageManager<P, C> {
    /// Creates a new MessageManager
    pub fn new(channel_config: &ChannelConfig<C>) -> Self {
        MessageManager {
            channel_config: channel_config.clone(),
            queued_outgoing_messages: VecDeque::new(),
            queued_incoming_messages: VecDeque::new(),
            sent_guaranteed_messages: HashMap::new(),
        }
    }

    pub fn generate_resend_messages(&mut self) {

    }

    // Outgoing Messages

    /// Returns whether the Manager has queued Messages that can be transmitted
    /// to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        return !self.queued_outgoing_messages.is_empty();
    }

    /// Gets the next queued Message to be transmitted
    pub fn pop_outgoing_message(&mut self, packet_index: PacketIndex) -> Option<(C, P)> {
        match self.queued_outgoing_messages.pop_front() {
            Some((channel_index, message)) => {
                let guaranteed = self.channel_config.settings(&channel_index).reliable();
                //place in transmission record if this is a guaranteed message
                if guaranteed {
                    if !self.sent_guaranteed_messages.contains_key(&packet_index) {
                        let sent_messages_list: Vec<(C, P)> = Vec::new();
                        self.sent_guaranteed_messages
                            .insert(packet_index, sent_messages_list);
                    }

                    if let Some(sent_messages_list) =
                        self.sent_guaranteed_messages.get_mut(&packet_index)
                    {
                        sent_messages_list.push((channel_index.clone(), message.clone()));
                    }
                }

                Some((channel_index, message))
            }
            None => None,
        }
    }

    /// Queues an Message to be transmitted to the remote host
    pub fn send_message(&mut self, channel_index: C, message: P) {
        self.queued_outgoing_messages.push_back((channel_index, message));
    }

    // Incoming Messages

    /// Get the most recently received Message
    pub fn pop_incoming_message(&mut self) -> Option<(C, P)> {
        return self.queued_incoming_messages.pop_front();
    }

    // MessageWriter

    /// Write into outgoing packet
    pub fn write_messages(
        &mut self,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        converter: &dyn NetEntityHandleConverter,
    ) {
        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                return;
            }

            let mut counter = BitCounter::new();
            write_list_header(&mut counter, &123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                return;
            }

            // Find how many messages will fit into the packet
            for (channel, message) in self.queued_outgoing_messages.iter() {
                MessageManager::<P, C>::write_message(&mut counter, channel, message, converter);
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }
            }
        }

        // Write header
        write_list_header(writer, &message_count);

        // Messages
        {
            for _ in 0..message_count {
                // Pop message
                let (message_channel, popped_message) =
                    self.pop_outgoing_message(packet_index).unwrap();

                // Write message
                MessageManager::<P, C>::write_message(
                    writer,
                    &message_channel,
                    &popped_message,
                    converter,
                );
            }
        }
    }

    /// Writes an Message into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_message<S: BitWrite>(
        writer: &mut S,
        channel: &C,
        message: &P,
        converter: &dyn NetEntityHandleConverter,
    ) {
        // write channel
        channel.ser(writer);

        // write message kind
        message.dyn_ref().kind().ser(writer);

        // write payload
        message.write(writer, converter);
    }

    // MessageReader
    pub fn read_messages(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    ) {
        let message_count = read_list_header(reader);
        self.process_message_data(reader, manifest, message_count, converter);
    }

    /// Given incoming packet data, read transmitted Messages and store them to
    /// be returned to the application
    fn process_message_data(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        message_count: u16,
        converter: &dyn NetEntityHandleConverter,
    ) {
        for _x in 0..message_count {
            // read channel
            let channel: C = C::de(reader).unwrap();

            // read message kind
            let component_kind: P::Kind = P::Kind::de(reader).unwrap();

            // read payload
            let new_message = manifest.create_replica(component_kind, reader, converter);

            self.queued_incoming_messages
                .push_back((channel, new_message));
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> PacketNotifiable for MessageManager<P, C> {
    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Messages in that packet.
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        self.sent_guaranteed_messages.remove(&packet_index);
    }

    /// Occurs when a packet has been notified as having been dropped. Queues up
    /// any guaranteed Messages that were lost in the packet for retransmission.
    fn notify_packet_dropped(&mut self, packet_index: PacketIndex) {
        if let Some(dropped_messages_list) = self.sent_guaranteed_messages.get(&packet_index) {
            for (channel, dropped_message) in dropped_messages_list.into_iter() {
                self.queued_outgoing_messages
                    .push_back((channel.clone(), dropped_message.clone()));
            }

            self.sent_guaranteed_messages.remove(&packet_index);
        }
    }
}
