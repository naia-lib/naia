use std::collections::HashMap;

use naia_shared::{
    BitWrite, BitWriter, ChannelKind, ChannelKinds, ChannelMode, ConstBitLength, MessageContainer,
    NetEntityHandleConverter, PacketIndex, PacketNotifiable, Protocol, Serde, ShortMessageIndex,
    Tick,
};

use super::channel_tick_buffer_sender::ChannelTickBufferSender;

pub struct TickBufferSender {
    channel_senders: HashMap<ChannelKind, ChannelTickBufferSender>,
    #[allow(clippy::type_complexity)]
    packet_to_channel_map: HashMap<PacketIndex, Vec<(ChannelKind, Vec<(Tick, ShortMessageIndex)>)>>,
}

impl TickBufferSender {
    pub fn new(channel_kinds: &ChannelKinds) -> Self {
        // initialize senders
        let mut channel_senders = HashMap::new();
        for (channel_kind, channel) in channel_kinds.channels() {
            if let ChannelMode::TickBuffered(settings) = &channel.mode {
                channel_senders
                    .insert(channel_kind, ChannelTickBufferSender::new(settings.clone()));
            }
        }

        Self {
            channel_senders,
            packet_to_channel_map: HashMap::new(),
        }
    }

    // Outgoing Messages

    pub fn send_message(
        &mut self,
        host_tick: &Tick,
        channel_kind: &ChannelKind,
        message: MessageContainer,
    ) {
        if let Some(channel) = self.channel_senders.get_mut(channel_kind) {
            channel.send_message(host_tick, message);
        }
    }

    pub fn collect_outgoing_messages(
        &mut self,
        client_sending_tick: &Tick,
        server_receivable_tick: &Tick,
    ) {
        for channel in self.channel_senders.values_mut() {
            channel.collect_outgoing_messages(client_sending_tick, server_receivable_tick);
        }
    }

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
        host_tick: &Tick,
        has_written: &mut bool,
    ) {
        for (channel_kind, channel) in &mut self.channel_senders {
            if !channel.has_messages() {
                continue;
            }

            // check that we can at least write a ChannelIndex and a MessageContinue bit
            let mut counter = writer.counter();
            counter.write_bits(<ChannelKind as ConstBitLength>::const_bit_length());
            counter.write_bit(false);

            if counter.overflowed() {
                break;
            }

            // write ChannelContinue bit
            true.ser(writer);

            // reserve MessageContinue bit
            writer.reserve_bits(1);

            // write ChannelIndex
            channel_kind.ser(&protocol.channel_kinds, writer);

            // write Messages
            if let Some(message_indices) = channel.write_messages(
                &protocol.message_kinds,
                converter,
                writer,
                host_tick,
                has_written,
            ) {
                self.packet_to_channel_map
                    .entry(packet_index)
                    .or_insert_with(Vec::new);
                let channel_list = self.packet_to_channel_map.get_mut(&packet_index).unwrap();
                channel_list.push((*channel_kind, message_indices));
            }

            // write MessageContinue finish bit, release
            false.ser(writer);
            writer.release_bits(1);
        }
    }
}

impl PacketNotifiable for TickBufferSender {
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(channel_list) = self.packet_to_channel_map.get(&packet_index) {
            for (channel_kind, message_indices) in channel_list {
                if let Some(channel) = self.channel_senders.get_mut(channel_kind) {
                    for (tick, message_index) in message_indices {
                        channel.notify_message_delivered(tick, message_index);
                    }
                }
            }
        }
    }
}
