use std::collections::{HashMap, VecDeque};

use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde};

use crate::{
    connection::packet_notifiable::PacketNotifiable,
    constants::MTU_SIZE_BITS,
    protocol::{
        entity_property::NetEntityHandleConverter, manifest::Manifest, protocolize::Protocolize,
    },
    types::{MessageId, PacketIndex},
};

use super::{
    channel_config::{ChannelConfig, ChannelIndex, ChannelMode},
    message_channel::MessageChannel,
    message_list_header::{read_list_header, write_list_header},
    ordered_reliable_channel::OrderedReliableChannel,
    unordered_reliable_channel::UnorderedReliableChannel,
    unordered_unreliable_channel::UnorderedUnreliableChannel,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct MessageManager<P: Protocolize, C: ChannelIndex> {
    channels: HashMap<C, Box<dyn MessageChannel<P, C>>>,
    outgoing_messages: VecDeque<(C, MessageId, P)>,
    packet_to_message_map: HashMap<PacketIndex, (C, MessageId)>,
}

impl<P: Protocolize, C: ChannelIndex> MessageManager<P, C> {
    /// Creates a new MessageManager
    pub fn new(channel_config: &ChannelConfig<C>) -> Self {
        // initialize all reliable channels
        let mut channels = HashMap::new();
        let all_channel_settings = channel_config.all_channels();
        for (channel_index, channel) in all_channel_settings {
            let new_channel: Option<Box<dyn MessageChannel<P, C>>> = match channel.mode {
                ChannelMode::UnorderedUnreliable => {
                    Some(Box::new(UnorderedUnreliableChannel::new(channel_index.clone())))
                }
                ChannelMode::UnorderedReliable(settings) => Some(Box::new(
                    UnorderedReliableChannel::new(channel_index.clone(), &settings),
                )),
                ChannelMode::OrderedReliable(settings) => Some(Box::new(
                    OrderedReliableChannel::new(channel_index.clone(), &settings),
                )),
                _ => None,
            };

            if new_channel.is_some() {
                channels.insert(channel_index.clone(), new_channel.unwrap());
            }
        }

        MessageManager {
            channels,
            outgoing_messages: VecDeque::new(),
            packet_to_message_map: HashMap::new(),
        }
    }

    pub fn collect_incoming_messages(&mut self) -> Vec<(C, P)> {
        let mut output: Vec<(C, P)> = Vec::new();
        for (_, channel) in &mut self.channels {
            channel.collect_incoming_messages(&mut output);
        }
        output
    }

    pub fn collect_outgoing_messages(&mut self, rtt_millis: &f32) {
        for (_, channel) in &mut self.channels {
            channel.collect_outgoing_messages(rtt_millis, &mut self.outgoing_messages);
        }
    }

    // Outgoing Messages

    /// Returns whether the Manager has queued Messages that can be transmitted
    /// to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        return self.outgoing_messages.len() > 0;
    }

    /// Queues an Message to be transmitted to the remote host
    pub fn send_message(&mut self, channel_index: C, message: P) {
        if let Some(channel) = self.channels.get_mut(&channel_index) {
            // reliable channels
            channel.send_message(message);
        }
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
            for (channel, message_id, message) in self.outgoing_messages.iter() {
                MessageManager::<P, C>::write_message(
                    &mut counter,
                    channel,
                    message_id,
                    message,
                    converter,
                );
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
                let (channel_index, message_id, popped_message) =
                    self.outgoing_messages.pop_front().unwrap();

                self.packet_to_message_map
                    .insert(packet_index, (channel_index.clone(), message_id));

                // Write message
                Self::write_message(
                    writer,
                    &channel_index,
                    &message_id,
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
        message_id: &MessageId,
        message: &P,
        converter: &dyn NetEntityHandleConverter,
    ) {
        // write channel
        channel.ser(writer);

        // write message id
        message_id.ser(writer);

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

            // read message id
            let message_id: MessageId = MessageId::de(reader).unwrap();

            // read message kind
            let component_kind: P::Kind = P::Kind::de(reader).unwrap();

            // read payload
            let new_message = manifest.create_replica(component_kind, reader, converter);

            if let Some(manager) = self.channels.get_mut(&channel) {
                manager.recv_message(message_id, new_message);
            }
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> PacketNotifiable for MessageManager<P, C> {
    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Messages in that packet.
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some((channel_index, message_id)) = self.packet_to_message_map.get(&packet_index) {
            if let Some(channel) = self.channels.get_mut(channel_index) {
                channel.notify_message_delivered(message_id);
            }
        }
    }

    fn notify_packet_dropped(&mut self, _: PacketIndex) {}
}
