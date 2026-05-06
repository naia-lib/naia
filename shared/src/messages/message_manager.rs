use std::collections::HashMap;

use log::error;
use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr};
use naia_socket_shared::Instant;

use crate::world::local::local_world_manager::LocalWorldManager;
use crate::{
    constants::FRAGMENTATION_LIMIT_BITS,
    messages::{
        channels::{
            channel::ChannelMode,
            channel::ChannelSettings,
            channel_kinds::{ChannelKind, ChannelKinds},
            receivers::{
                channel_receiver::MessageChannelReceiver,
                ordered_reliable_receiver::OrderedReliableReceiver,
                sequenced_reliable_receiver::SequencedReliableReceiver,
                sequenced_unreliable_receiver::SequencedUnreliableReceiver,
                unordered_reliable_receiver::UnorderedReliableReceiver,
                unordered_unreliable_receiver::UnorderedUnreliableReceiver,
            },
            senders::{
                channel_sender::MessageChannelSender, message_fragmenter::MessageFragmenter,
                reliable_message_sender::ReliableMessageSender, request_sender::LocalResponseId,
                sequenced_unreliable_sender::SequencedUnreliableSender,
                unordered_unreliable_sender::UnorderedUnreliableSender,
            },
        },
        message_container::MessageContainer,
        request::GlobalRequestId,
    },
    types::{HostType, MessageIndex, PacketIndex},
    world::{
        entity::entity_converters::LocalEntityAndGlobalEntityConverterMut,
        remote::remote_entity_waitlist::RemoteEntityWaitlist,
    },
    LocalEntityAndGlobalEntityConverter, MessageKinds, PacketNotifiable,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct MessageManager {
    channel_senders: HashMap<ChannelKind, Box<dyn MessageChannelSender>>,
    channel_receivers: HashMap<ChannelKind, Box<dyn MessageChannelReceiver>>,
    channel_settings: HashMap<ChannelKind, ChannelSettings>,
    packet_to_message_map: HashMap<PacketIndex, Vec<(ChannelKind, Vec<MessageIndex>)>>,
    message_fragmenter: MessageFragmenter,
}

impl MessageManager {
    /// Creates a new MessageManager
    pub fn new(host_type: HostType, channel_kinds: &ChannelKinds) -> Self {
        // initialize all reliable channels

        // initialize senders
        let mut channel_senders = HashMap::<ChannelKind, Box<dyn MessageChannelSender>>::new();
        for (channel_kind, channel_settings) in channel_kinds.channels() {
            //info!("initialize senders for channel: {:?}", channel_kind);
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
                        Box::new(ReliableMessageSender::new(settings.rtt_resend_factor)),
                    );
                }
                ChannelMode::TickBuffered(_) => {
                    // Tick buffered channel uses another manager, skip
                }
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
                        channel_kind,
                        Box::new(UnorderedUnreliableReceiver::new()),
                    );
                }
                ChannelMode::SequencedUnreliable => {
                    channel_receivers.insert(
                        channel_kind,
                        Box::new(SequencedUnreliableReceiver::new()),
                    );
                }
                ChannelMode::UnorderedReliable(_) => {
                    channel_receivers.insert(
                        channel_kind,
                        Box::new(UnorderedReliableReceiver::new()),
                    );
                }
                ChannelMode::SequencedReliable(_) => {
                    channel_receivers.insert(
                        channel_kind,
                        Box::new(SequencedReliableReceiver::new()),
                    );
                }
                ChannelMode::OrderedReliable(_) => {
                    channel_receivers.insert(
                        channel_kind,
                        Box::new(OrderedReliableReceiver::new()),
                    );
                }
                ChannelMode::TickBuffered(_) => {
                    // Tick buffered channel uses another manager, skip
                }
            };
        }

        // initialize settings
        let mut channel_settings_map = HashMap::new();
        for (channel_kind, channel_settings) in channel_kinds.channels() {
            channel_settings_map.insert(channel_kind, channel_settings);
        }

        Self {
            channel_senders,
            channel_receivers,
            channel_settings: channel_settings_map,
            packet_to_message_map: HashMap::new(),
            message_fragmenter: MessageFragmenter::new(),
        }
    }

    // Outgoing Messages

    /// Queues an Message to be transmitted to the remote host
    pub fn send_message(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        channel_kind: &ChannelKind,
        message: MessageContainer,
    ) {
        let Some(channel) = self.channel_senders.get_mut(channel_kind) else {
            panic!("Channel not configured correctly! Cannot send message.");
        };

        let message_bit_length = message.bit_length(message_kinds, converter);
        if message_bit_length > FRAGMENTATION_LIMIT_BITS {
            let Some(settings) = self.channel_settings.get(channel_kind) else {
                panic!("Channel not configured correctly! Cannot send message.");
            };
            if !settings.reliable() {
                error!("ERROR: Attempting to send Message above the fragmentation size limit over an unreliable Message channel! Slim down the size of your Message, or send this Message through a reliable message channel.");
                return;
            }

            // Now fragment this message ...
            let messages =
                self.message_fragmenter
                    .fragment_message(message_kinds, converter, message);
            for message_fragment in messages {
                channel.send_message(message_fragment);
            }
        } else {
            channel.send_message(message);
        }
    }

    pub fn send_request(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        channel_kind: &ChannelKind,
        global_request_id: GlobalRequestId,
        request: MessageContainer,
    ) {
        let Some(channel) = self.channel_senders.get_mut(channel_kind) else {
            panic!("Channel not configured correctly! Cannot send message.");
        };
        channel.send_outgoing_request(message_kinds, converter, global_request_id, request);
    }

    pub fn send_response(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        channel_kind: &ChannelKind,
        local_response_id: LocalResponseId,
        response: MessageContainer,
    ) {
        let Some(channel) = self.channel_senders.get_mut(channel_kind) else {
            panic!("Channel not configured correctly! Cannot send message.");
        };
        channel.send_outgoing_response(message_kinds, converter, local_response_id, response);
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
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        has_written: &mut bool,
    ) {
        // Phase A: walk channels in descending criticality order so High
        // (e.g. TickBuffered) wins packet space over Normal wins over Low
        // under tight budgets. Stable sort preserves equal-gain order.
        // Reverse bits of base_gain into an ordering key so higher base_gain
        // sorts first; ties broken by ChannelKind order (stable).
        let mut ordered: Vec<(ChannelKind, f32)> = self
            .channel_senders
            .keys()
            .map(|k| {
                let gain = self
                    .channel_settings
                    .get(k)
                    .map(|s| s.criticality.base_gain())
                    .unwrap_or(1.0);
                (*k, gain)
            })
            .collect();
        ordered.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });

        for (channel_kind, _gain) in &ordered {
            let channel = self.channel_senders.get_mut(channel_kind).unwrap();
            if !channel.has_messages() {
                continue;
            }

            // check that we can at least write a ChannelIndex and a MessageContinue bit
            let mut counter = writer.counter();
            // reserve MessageContinue bit
            counter.write_bit(false);
            // write ChannelContinue bit
            counter.write_bit(false);
            // write ChannelIndex (variable-width — count the actual bits this
            // channel will take rather than a const upper bound)
            channel_kind.ser(channel_kinds, &mut counter);
            if counter.overflowed() {
                break;
            }

            // reserve MessageContinue bit
            writer.reserve_bits(1);
            // write ChannelContinue bit
            true.ser(writer);
            // write ChannelIndex
            channel_kind.ser(channel_kinds, writer);
            // write Messages
            if let Some(message_indices) =
                channel.write_messages(message_kinds, converter, writer, has_written)
            {
                self.packet_to_message_map
                    .entry(packet_index)
                    .or_default();
                let channel_list = self.packet_to_message_map.get_mut(&packet_index).unwrap();
                channel_list.push((*channel_kind, message_indices));
            }

            // write MessageContinue finish bit, release
            writer.release_bits(1);
            false.ser(writer);
        }

        // write ChannelContinue finish bit, release
        writer.release_bits(1);
        false.ser(writer);
    }

    // Incoming Messages

    pub fn read_messages(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        local_world_manager: &mut LocalWorldManager,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            let message_continue = bool::de(reader)?;
            if !message_continue {
                break;
            }

            // read channel id
            let channel_kind = ChannelKind::de(channel_kinds, reader)?;

            // continue read inside channel
            let channel = self.channel_receivers.get_mut(&channel_kind).unwrap();
            channel.read_messages(message_kinds, local_world_manager, reader)?;
        }

        Ok(())
    }

    /// Retrieve all messages from the channel buffers
    pub fn receive_messages(
        &mut self,
        message_kinds: &MessageKinds,
        now: &Instant,
        entity_converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity_waitlist: &mut RemoteEntityWaitlist,
    ) -> Vec<(ChannelKind, Vec<MessageContainer>)> {
        let mut output = Vec::new();
        // TODO: shouldn't we have a priority mechanisms between channels?
        for (channel_kind, channel) in &mut self.channel_receivers {
            let messages =
                channel.receive_messages(message_kinds, now, entity_waitlist, entity_converter);
            output.push((*channel_kind, messages));
        }
        output
    }

    /// Retrieve all requests from the channel buffers
    pub fn receive_requests_and_responses(
        &mut self,
    ) -> (
        Vec<(ChannelKind, Vec<(LocalResponseId, MessageContainer)>)>,
        Vec<(GlobalRequestId, MessageContainer)>,
    ) {
        let mut request_output = Vec::new();
        let mut response_output = Vec::new();
        for (channel_kind, channel) in &mut self.channel_receivers {
            if !self
                .channel_settings
                .get(channel_kind)
                .unwrap()
                .can_request_and_respond()
            {
                continue;
            }

            let (requests, responses) = channel.receive_requests_and_responses();
            if !requests.is_empty() {
                request_output.push((*channel_kind, requests));
            }

            if !responses.is_empty() {
                let Some(channel_sender) = self.channel_senders.get_mut(channel_kind) else {
                    panic!(
                        "Channel not configured correctly! Cannot send message on channel: {:?}",
                        channel_kind
                    );
                };
                for (local_request_id, response) in responses {
                    let global_request_id = channel_sender
                        .process_incoming_response(&local_request_id)
                        .unwrap();
                    response_output.push((global_request_id, response));
                }
            }
        }
        (request_output, response_output)
    }
}

impl PacketNotifiable for MessageManager {
    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Messages in that packet.
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(channel_list) = self.packet_to_message_map.get(&packet_index) {
            for (channel_kind, message_indices) in channel_list {
                if let Some(channel) = self.channel_senders.get_mut(channel_kind) {
                    for message_index in message_indices {
                        channel.notify_message_delivered(message_index);
                    }
                }
            }
        }
    }
}
