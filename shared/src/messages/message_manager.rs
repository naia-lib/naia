use std::collections::HashMap;

use naia_serde::{BitReader, BitWriter, Serde, UnsignedVariableInteger};

use crate::{
    connection::packet_notifiable::PacketNotifiable,
    protocol::{
        entity_property::NetEntityHandleConverter, manifest::Manifest, protocolize::Protocolize,
    },
    types::{MessageId, PacketIndex},
};

use super::{
    channel_config::{ChannelConfig, ChannelIndex, ChannelMode},
    message_channel::{ChannelReceiver, ChannelSender},
    ordered_reliable_receiver::OrderedReliableReceiver,
    reliable_sender::ReliableSender,
    unordered_reliable_receiver::UnorderedReliableReceiver,
    unordered_unreliable_receiver::UnorderedUnreliableReceiver,
    unordered_unreliable_sender::UnorderedUnreliableSender,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct MessageManager<P: Protocolize, C: ChannelIndex> {
    channel_senders: HashMap<C, Box<dyn ChannelSender<P>>>,
    channel_receivers: HashMap<C, Box<dyn ChannelReceiver<P>>>,
    packet_to_message_map: HashMap<PacketIndex, Vec<(C, Vec<MessageId>)>>,
}

impl<P: Protocolize, C: ChannelIndex> MessageManager<P, C> {

    /// Creates a new MessageManager
    pub fn new(channel_config: &ChannelConfig<C>) -> Self {

        // initialize all reliable channels
        let mut channel_senders = HashMap::<C, Box<dyn ChannelSender<P>>>::new();
        let mut channel_receivers = HashMap::<C, Box<dyn ChannelReceiver<P>>>::new();

        for channel_index in &channel_config.channels().vec {
            let channel = channel_config.channels().map.get(channel_index).unwrap();
            match &channel.mode {
                ChannelMode::UnorderedUnreliable => {
                    channel_senders.insert(
                        channel_index.clone(),
                        Box::new(UnorderedUnreliableSender::new()),
                    );
                    channel_receivers.insert(
                        channel_index.clone(),
                        Box::new(UnorderedUnreliableReceiver::new()),
                    );
                }
                ChannelMode::UnorderedReliable(settings) => {
                    channel_senders.insert(
                        channel_index.clone(),
                        Box::new(ReliableSender::new(&settings)),
                    );
                    channel_receivers.insert(
                        channel_index.clone(),
                        Box::new(UnorderedReliableReceiver::new()),
                    );
                }
                ChannelMode::OrderedReliable(settings) => {
                    channel_senders.insert(
                        channel_index.clone(),
                        Box::new(ReliableSender::new(&settings)),
                    );
                    channel_receivers.insert(
                        channel_index.clone(),
                        Box::new(OrderedReliableReceiver::new()),
                    );
                }
                _ => {}
            };
        }

        MessageManager {
            channel_senders,
            channel_receivers,
            packet_to_message_map: HashMap::new(),
        }
    }

    // Outgoing Messages

    /// Queues an Message to be transmitted to the remote host
    pub fn send_message(&mut self, channel_index: C, message: P) {
        if let Some(channel) = self.channel_senders.get_mut(&channel_index) {
            channel.send_message(message);
        }
    }

    pub fn collect_outgoing_messages(&mut self, rtt_millis: &f32) {
        for (_, channel) in &mut self.channel_senders {
            channel.collect_messages(rtt_millis);
        }
    }

    /// Returns whether the Manager has queued Messages that can be transmitted
    /// to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        for (_, channel) in &self.channel_senders {
            if channel.has_messages() {
                return true;
            }
        }
        return false;
    }

    pub fn write_messages(
        &mut self,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        converter: &dyn NetEntityHandleConverter,
    ) {
        let mut channels_to_write = Vec::new();
        for (channel_index, channel) in &self.channel_senders {
            if channel.has_messages() {
                channels_to_write.push(channel_index.clone());
            }
        }

        // write channel count
        UnsignedVariableInteger::<3>::new(channels_to_write.len() as u64).ser(writer);

        for channel_index in channels_to_write {
            let channel = self.channel_senders.get_mut(&channel_index).unwrap();

            // write channel index
            channel_index.ser(writer);

            if let Some(message_ids) = channel.write_messages(converter, writer) {
                if !self.packet_to_message_map.contains_key(&packet_index) {
                    self.packet_to_message_map.insert(packet_index, Vec::new());
                }
                let channel_list = self.packet_to_message_map.get_mut(&packet_index).unwrap();
                channel_list.push((channel_index.clone(), message_ids));
            }
        }
    }

    // Incoming Messages

    pub fn read_messages(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter) {

        // read channel count
        let channel_count = UnsignedVariableInteger::<3>::de(reader).unwrap().get();

        for _ in 0..channel_count {
            // read channel index
            let channel_index = C::de(reader).unwrap();
            if let Some(channel) = self.channel_receivers.get_mut(&channel_index) {
                channel.read_messages(reader, manifest, converter);
            }
        }
    }

    pub fn receive_messages(&mut self) -> Vec<(C, P)> {
        let mut output = Vec::new();
        for (channel_index, channel) in &mut self.channel_receivers {
            let mut messages = channel.receive_messages();
            for message in messages.drain(..) {
                output.push((channel_index.clone(), message));
            }
        }
        return output;
    }
}

impl<P: Protocolize, C: ChannelIndex> PacketNotifiable for MessageManager<P, C> {
    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Messages in that packet.
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(channel_list) = self.packet_to_message_map.get(&packet_index) {
            for (channel_index, message_ids) in channel_list {
                if let Some(channel) = self.channel_senders.get_mut(channel_index) {
                    for message_id in message_ids {
                        channel.notify_message_delivered(message_id);
                    }
                }
            }
        }
    }

    fn notify_packet_dropped(&mut self, _: PacketIndex) {}
}
