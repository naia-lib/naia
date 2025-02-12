use std::collections::HashMap;

use naia_shared::{
    BitWrite, BitWriter, ChannelKind, ChannelKinds, ChannelMode, ConstBitLength,
    EntityConverterMut, LocalWorldManager, MessageContainer, PacketIndex, PacketNotifiable,
    Protocol, Serde, ShortMessageIndex, Tick,
};

use super::channel_tick_buffer_sender::ChannelTickBufferSender;
use crate::world::global_world_manager::GlobalWorldManager;

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

    pub fn collect_messages(&mut self, client_sending_tick: &Tick, server_receivable_tick: &Tick) {
        for channel in self.channel_senders.values_mut() {
            channel.collect_messages(client_sending_tick, server_receivable_tick);
        }
    }

    pub fn has_messages(&self) -> bool {
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
        global_world_manager: &GlobalWorldManager,
        local_world_manager: &mut LocalWorldManager,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        host_tick: &Tick,
        has_written: &mut bool,
    ) {
        let mut converter = EntityConverterMut::new(global_world_manager, local_world_manager);

        for (channel_kind, channel) in &mut self.channel_senders {
            if !channel.has_messages() {
                continue;
            }

            // check that we can at least write a ChannelIndex and a MessageContinue bit
            let mut counter = writer.counter();
            // reserve MessageContinue bit
            true.ser(&mut counter);
            // write ChannelContinue bit
            true.ser(&mut counter);
            // write ChannelIndex
            counter.count_bits(<ChannelKind as ConstBitLength>::const_bit_length());

            if counter.overflowed() {
                break;
            }

            // reserve MessageContinue bit
            writer.reserve_bits(1);
            // write ChannelContinue bit
            true.ser(writer);
            // write ChannelIndex
            channel_kind.ser(&protocol.channel_kinds, writer);
            // write Messages
            if let Some(message_indices) = channel.write_messages(
                &protocol.message_kinds,
                &mut converter,
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
            writer.release_bits(1);
            false.ser(writer);
        }

        // write ChannelContinue finish bit, release
        writer.release_bits(1);
        false.ser(writer);
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
