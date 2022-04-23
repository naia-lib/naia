use std::collections::HashMap;

use naia_serde::{BitReader, BitWriter, Serde, UnsignedVariableInteger};
use naia_socket_shared::Instant;

use crate::{
    connection::packet_notifiable::PacketNotifiable,
    protocol::protocolize::Protocolize,
    types::{HostType, MessageId, PacketIndex},
};

use super::{
    channel_config::{ChannelConfig, ChannelIndex, ChannelMode},
    message_channel::{ChannelReader, ChannelReceiver, ChannelSender, ChannelWriter},
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
    pub fn new(host_type: HostType, channel_config: &ChannelConfig<C>) -> Self {
        // initialize all reliable channels

        // initialize senders
        let mut channel_senders = HashMap::<C, Box<dyn ChannelSender<P>>>::new();
        for (channel_index, channel) in channel_config.channels() {
            match &host_type {
                HostType::Server => {
                    if !channel.can_send_to_client() {
                        continue;
                    }
                }
                HostType::Client => {
                    if !channel.can_send_to_server() {
                        continue;
                    }
                }
            }

            match &channel.mode {
                ChannelMode::UnorderedUnreliable => {
                    channel_senders.insert(
                        channel_index.clone(),
                        Box::new(UnorderedUnreliableSender::new()),
                    );
                }
                ChannelMode::UnorderedReliable(settings) => {
                    channel_senders.insert(
                        channel_index.clone(),
                        Box::new(ReliableSender::new(settings.rtt_resend_factor)),
                    );
                }
                ChannelMode::OrderedReliable(settings) => {
                    channel_senders.insert(
                        channel_index.clone(),
                        Box::new(ReliableSender::new(settings.rtt_resend_factor)),
                    );
                }
                _ => {}
            };
        }

        // initialize receivers
        let mut channel_receivers = HashMap::<C, Box<dyn ChannelReceiver<P>>>::new();
        for (channel_index, channel) in channel_config.channels() {
            match &host_type {
                HostType::Server => {
                    if !channel.can_send_to_server() {
                        continue;
                    }
                }
                HostType::Client => {
                    if !channel.can_send_to_client() {
                        continue;
                    }
                }
            }

            match &channel.mode {
                ChannelMode::UnorderedUnreliable => {
                    channel_receivers.insert(
                        channel_index.clone(),
                        Box::new(UnorderedUnreliableReceiver::new()),
                    );
                }
                ChannelMode::UnorderedReliable(_) => {
                    channel_receivers.insert(
                        channel_index.clone(),
                        Box::new(UnorderedReliableReceiver::default()),
                    );
                }
                ChannelMode::OrderedReliable(_) => {
                    channel_receivers.insert(
                        channel_index.clone(),
                        Box::new(OrderedReliableReceiver::default()),
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

    pub fn collect_outgoing_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        for channel in self.channel_senders.values_mut() {
            channel.collect_messages(now, rtt_millis);
        }
    }

    /// Returns whether the Manager has queued Messages that can be transmitted
    /// to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        for channel in self.channel_senders.values() {
            if channel.has_messages() {
                return true;
            }
        }
        false
    }

    pub fn write_messages(
        &mut self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut BitWriter,
        packet_index: PacketIndex,
    ) {
        let mut channels_to_write = Vec::new();
        for (channel_index, channel) in &self.channel_senders {
            if channel.has_messages() {
                channels_to_write.push(channel_index.clone());
            }
        }

        // write channel count
        UnsignedVariableInteger::<3>::new(channels_to_write.len() as u64).ser(bit_writer);

        for channel_index in channels_to_write {
            let channel = self.channel_senders.get_mut(&channel_index).unwrap();

            // write channel index
            channel_index.ser(bit_writer);

            if let Some(message_ids) = channel.write_messages(channel_writer, bit_writer) {
                self.packet_to_message_map
                    .entry(packet_index)
                    .or_insert_with(Vec::new);
                let channel_list = self.packet_to_message_map.get_mut(&packet_index).unwrap();
                channel_list.push((channel_index.clone(), message_ids));
            }
        }
    }

    // Incoming Messages

    pub fn read_messages(
        &mut self,
        channel_reader: &dyn ChannelReader<P>,
        bit_reader: &mut BitReader,
    ) {
        // read channel count
        let channel_count = UnsignedVariableInteger::<3>::de(bit_reader).unwrap().get();

        for _ in 0..channel_count {
            // read channel index
            let channel_index = C::de(bit_reader).unwrap();

            // continue read inside channel
            if let Some(channel) = self.channel_receivers.get_mut(&channel_index) {
                channel.read_messages(channel_reader, bit_reader);
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
        output
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
}
