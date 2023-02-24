use std::collections::HashMap;

use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr};
use naia_socket_shared::Instant;

use crate::{
    connection::packet_notifiable::PacketNotifiable,
    messages::{
        channel_kinds::{ChannelKind, ChannelKinds},
        message_channel::{MessageChannelReceiver, MessageChannelSender},
    },
    types::{HostType, MessageIndex, PacketIndex},
    Message, NetEntityHandleConverter, Protocol,
};

use super::{
    channel::ChannelMode, ordered_reliable_receiver::OrderedReliableReceiver,
    reliable_sender::ReliableSender, sequenced_reliable_receiver::SequencedReliableReceiver,
    sequenced_unreliable_receiver::SequencedUnreliableReceiver,
    sequenced_unreliable_sender::SequencedUnreliableSender,
    unordered_reliable_receiver::UnorderedReliableReceiver,
    unordered_unreliable_receiver::UnorderedUnreliableReceiver,
    unordered_unreliable_sender::UnorderedUnreliableSender,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct MessageManager {
    channel_senders: HashMap<ChannelKind, Box<dyn MessageChannelSender>>,
    channel_receivers: HashMap<ChannelKind, Box<dyn MessageChannelReceiver>>,
    packet_to_message_map: HashMap<PacketIndex, Vec<(ChannelKind, Vec<MessageIndex>)>>,
}

impl MessageManager {
    /// Creates a new MessageManager
    pub fn new(host_type: HostType, channel_kinds: &ChannelKinds) -> Self {
        // initialize all reliable channels

        // initialize senders
        let mut channel_senders = HashMap::<ChannelKind, Box<dyn MessageChannelSender>>::new();
        for (channel_kind, channel_settings) in channel_kinds.channels() {
            match &host_type {
                HostType::Server => {
                    if !channel_settings.can_send_to_client() {
                        continue;
                    }
                }
                HostType::Client => {
                    if !channel_settings.can_send_to_server() {
                        continue;
                    }
                }
            }

            match &channel_settings.mode {
                ChannelMode::UnorderedUnreliable => {
                    channel_senders
                        .insert(channel_kind, Box::new(UnorderedUnreliableSender::new()));
                }
                ChannelMode::SequencedUnreliable => {
                    channel_senders
                        .insert(channel_kind, Box::new(SequencedUnreliableSender::new()));
                }
                ChannelMode::UnorderedReliable(settings)
                | ChannelMode::SequencedReliable(settings)
                | ChannelMode::OrderedReliable(settings) => {
                    channel_senders.insert(
                        channel_kind,
                        Box::new(ReliableSender::<Box<dyn Message>>::new(
                            settings.rtt_resend_factor,
                        )),
                    );
                }
                _ => {}
            };
        }

        // initialize receivers
        let mut channel_receivers = HashMap::<ChannelKind, Box<dyn MessageChannelReceiver>>::new();
        for (channel_kind, channel_settings) in channel_kinds.channels() {
            match &host_type {
                HostType::Server => {
                    if !channel_settings.can_send_to_server() {
                        continue;
                    }
                }
                HostType::Client => {
                    if !channel_settings.can_send_to_client() {
                        continue;
                    }
                }
            }

            match &channel_settings.mode {
                ChannelMode::UnorderedUnreliable => {
                    channel_receivers.insert(
                        channel_kind.clone(),
                        Box::new(UnorderedUnreliableReceiver::new()),
                    );
                }
                ChannelMode::SequencedUnreliable => {
                    channel_receivers.insert(
                        channel_kind.clone(),
                        Box::new(SequencedUnreliableReceiver::new()),
                    );
                }
                ChannelMode::UnorderedReliable(_) => {
                    channel_receivers.insert(
                        channel_kind.clone(),
                        Box::new(UnorderedReliableReceiver::default()),
                    );
                }
                ChannelMode::SequencedReliable(_) => {
                    channel_receivers.insert(
                        channel_kind.clone(),
                        Box::new(SequencedReliableReceiver::default()),
                    );
                }
                ChannelMode::OrderedReliable(_) => {
                    channel_receivers.insert(
                        channel_kind.clone(),
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
    pub fn send_message(&mut self, channel_kind: &ChannelKind, message: Box<dyn Message>) {
        if let Some(channel) = self.channel_senders.get_mut(channel_kind) {
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
        protocol: &Protocol,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        has_written: &mut bool,
    ) {
        for (channel_index, channel) in &mut self.channel_senders {
            if !channel.has_messages() {
                continue;
            }

            // check that we can at least write a ChannelIndex and a MessageContinue bit
            let mut counter = writer.counter();
            channel_index.ser(&protocol.channel_kinds, &mut counter);
            counter.write_bit(false);

            if counter.overflowed() {
                break;
            }

            // write ChannelContinue bit
            true.ser(writer);

            // reserve MessageContinue bit
            writer.reserve_bits(1);

            // write ChannelIndex
            channel_index.ser(&protocol.channel_kinds, writer);

            // write Messages
            if let Some(message_indexs) =
                channel.write_messages(&protocol.message_kinds, converter, writer, has_written)
            {
                self.packet_to_message_map
                    .entry(packet_index)
                    .or_insert_with(Vec::new);
                let channel_list = self.packet_to_message_map.get_mut(&packet_index).unwrap();
                channel_list.push((channel_index.clone(), message_indexs));
            }

            // write MessageContinue finish bit, release
            false.ser(writer);
            writer.release_bits(1);
        }
    }

    // Incoming Messages

    pub fn read_messages(
        &mut self,
        protocol: &Protocol,
        converter: &dyn NetEntityHandleConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            let message_continue = bool::de(reader)?;
            if !message_continue {
                break;
            }

            // read channel id
            let channel_kind = ChannelKind::de(&protocol.channel_kinds, reader)?;

            // continue read inside channel
            let channel = self.channel_receivers.get_mut(&channel_kind).unwrap();
            channel.read_messages(&protocol.message_kinds, converter, reader)?;
        }

        Ok(())
    }

    /// Retrieve all messages from the channel buffers
    pub fn receive_messages(&mut self) -> Vec<(ChannelKind, Box<dyn Message>)> {
        let mut output = Vec::new();
        // TODO: shouldn't we have a priority mechanisms between channels?
        for (channel_kind, channel) in &mut self.channel_receivers {
            let messages = channel.receive_messages();
            for message in messages {
                output.push((channel_kind.clone(), message));
            }
        }
        output
    }
}

impl PacketNotifiable for MessageManager {
    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Messages in that packet.
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(channel_list) = self.packet_to_message_map.get(&packet_index) {
            for (channel_index, message_indexs) in channel_list {
                if let Some(channel) = self.channel_senders.get_mut(channel_index) {
                    for message_index in message_indexs {
                        channel.notify_message_delivered(message_index);
                    }
                }
            }
        }
    }
}
