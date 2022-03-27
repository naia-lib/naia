use std::collections::HashMap;

use naia_serde::{BitReader, BitWriter};

use crate::{
    connection::packet_notifiable::PacketNotifiable,
    protocol::{
        entity_property::NetEntityHandleConverter, manifest::Manifest, protocolize::Protocolize,
    },
    types::{MessageId, PacketIndex},
    vecmap::VecMap,
};

use super::{
    channel_config::{ChannelConfig, ChannelIndex, ChannelMode},
    message_channel::MessageChannel,
    ordered_reliable_channel::OrderedReliableChannel,
    unordered_reliable_channel::UnorderedReliableChannel,
    unordered_unreliable_channel::UnorderedUnreliableChannel,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct MessageManager<P: Protocolize, C: ChannelIndex> {
    channels: VecMap<C, Box<dyn MessageChannel<P>>>,
    packet_to_message_map: HashMap<PacketIndex, Vec<(C, Vec<MessageId>)>>,
}

impl<P: Protocolize, C: ChannelIndex> MessageManager<P, C> {
    /// Creates a new MessageManager
    pub fn new(channel_config: &ChannelConfig<C>) -> Self {
        // initialize all reliable channels
        let mut channels = VecMap::new();
        for channel_index in &channel_config.channels().vec {
            let channel = channel_config.channels().map.get(channel_index).unwrap();
            let new_channel: Option<Box<dyn MessageChannel<P>>> = match &channel.mode {
                ChannelMode::UnorderedUnreliable => Some(Box::new(
                    UnorderedUnreliableChannel::new(),
                )),
                ChannelMode::UnorderedReliable(settings) => Some(Box::new(
                    UnorderedReliableChannel::new(&settings),
                )),
                ChannelMode::OrderedReliable(settings) => Some(Box::new(
                    OrderedReliableChannel::new(&settings),
                )),
                _ => None,
            };

            if new_channel.is_some() {
                channels.dual_insert(channel_index.clone(), new_channel.unwrap());
            }
        }

        MessageManager {
            channels,
            packet_to_message_map: HashMap::new(),
        }
    }

    pub fn collect_incoming_messages(&mut self) -> Vec<(C, P)> {
        let mut output = Vec::new();
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get_mut(channel_index).unwrap();
            let mut messages = channel.collect_incoming_messages();
            for message in messages.drain(..) {
                output.push((channel_index.clone(), message));
            }
        }
        return output;
    }

    pub fn collect_outgoing_messages(&mut self, rtt_millis: &f32) {
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get_mut(channel_index).unwrap();
            channel.collect_outgoing_messages(rtt_millis);
        }
    }

    // Outgoing Messages

    /// Returns whether the Manager has queued Messages that can be transmitted
    /// to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get(channel_index).unwrap();
            if channel.has_outgoing_messages() {
                return true;
            }
        }
        return false;
    }

    /// Queues an Message to be transmitted to the remote host
    pub fn send_message(&mut self, channel_index: C, message: P) {
        if let Some(channel) = self.channels.map.get_mut(&channel_index) {
            channel.send_message(message);
        }
    }

    // MessageWriter

    pub fn write_messages(
        &mut self,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        converter: &dyn NetEntityHandleConverter,
    ) {
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get_mut(channel_index).unwrap();
            if let Some(message_ids) = channel.write_messages(converter, writer) {
                if !self.packet_to_message_map.contains_key(&packet_index) {
                    self.packet_to_message_map.insert(packet_index, Vec::new());
                }
                let channel_list = self.packet_to_message_map.get_mut(&packet_index).unwrap();
                channel_list.push((channel_index.clone(), message_ids));
            }
        }
    }

    // MessageReader
    pub fn read_messages(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    ) {
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get_mut(channel_index).unwrap();
            channel.read_messages(reader, manifest, converter);
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> PacketNotifiable for MessageManager<P, C> {
    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Messages in that packet.
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(channel_list) = self.packet_to_message_map.get(&packet_index) {
            for (channel_index, message_ids) in channel_list {
                if let Some(channel) = self.channels.map.get_mut(channel_index) {
                    for message_id in message_ids {
                        channel.notify_message_delivered(message_id);
                    }
                }
            }
        }
    }

    fn notify_packet_dropped(&mut self, _: PacketIndex) {}
}
