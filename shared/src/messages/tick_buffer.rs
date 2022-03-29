use std::collections::HashMap;

use naia_serde::{BitReader, BitWriter, Serde, UnsignedVariableInteger};

use crate::{
    connection::packet_notifiable::PacketNotifiable,
    protocol::protocolize::Protocolize,
    types::{PacketIndex, ShortMessageId, Tick},
};

use super::{
    channel_config::{ChannelConfig, ChannelIndex, ChannelMode},
    channel_tick_buffer_receiver::ChannelTickBufferReceiver,
    channel_tick_buffer_sender::ChannelTickBufferSender,
    message_channel::{ChannelReader, ChannelWriter},
};

pub struct TickBuffer<P: Protocolize, C: ChannelIndex> {
    channel_senders: HashMap<C, ChannelTickBufferSender<P>>,
    channel_receivers: HashMap<C, ChannelTickBufferReceiver<P>>,
    packet_to_channel_map: HashMap<PacketIndex, Vec<(C, Vec<(Tick, ShortMessageId)>)>>,
}

impl<P: Protocolize, C: ChannelIndex> TickBuffer<P, C> {
    pub fn new(channel_config: &ChannelConfig<C>) -> Self {
        // initialize all tick buffer channels
        let mut channel_senders = HashMap::new();
        let mut channel_receivers = HashMap::new();

        for (channel_index, channel) in channel_config.channels() {
            match &channel.mode {
                ChannelMode::TickBuffered(settings) => {
                    channel_senders.insert(
                        channel_index.clone(),
                        ChannelTickBufferSender::new(&settings),
                    );
                    channel_receivers
                        .insert(channel_index.clone(), ChannelTickBufferReceiver::new());
                }
                _ => {}
            }
        }

        TickBuffer {
            channel_senders,
            channel_receivers,
            packet_to_channel_map: HashMap::new(),
        }
    }

    // Outgoing Messages

    pub fn send_message(&mut self, host_tick: &Tick, channel_index: C, message: P) {
        if let Some(channel) = self.channel_senders.get_mut(&channel_index) {
            channel.send_message(host_tick, message);
        }
    }

    pub fn collect_outgoing_messages(
        &mut self,
        client_sending_tick: &Tick,
        server_receivable_tick: &Tick,
    ) {
        for (_, channel) in &mut self.channel_senders {
            channel.collect_outgoing_messages(client_sending_tick, server_receivable_tick);
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        for (_, channel) in &self.channel_senders {
            if channel.has_outgoing_messages() {
                return true;
            }
        }
        return false;
    }

    pub fn write_messages(
        &mut self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut BitWriter,
        packet_index: PacketIndex,
        host_tick: &Tick,
    ) {
        let mut channels_to_write = Vec::new();
        for (channel_index, channel) in &self.channel_senders {
            if channel.has_outgoing_messages() {
                channels_to_write.push(channel_index.clone());
            }
        }

        // write channel count
        UnsignedVariableInteger::<3>::new(channels_to_write.len() as u64).ser(bit_writer);

        for channel_index in channels_to_write {
            let channel = self.channel_senders.get_mut(&channel_index).unwrap();

            // write channel index
            channel_index.ser(bit_writer);

            if let Some(message_ids) = channel.write_messages(channel_writer, bit_writer, host_tick)
            {
                if !self.packet_to_channel_map.contains_key(&packet_index) {
                    self.packet_to_channel_map.insert(packet_index, Vec::new());
                }
                let channel_list = self.packet_to_channel_map.get_mut(&packet_index).unwrap();
                channel_list.push((channel_index.clone(), message_ids));
            }
        }
    }

    // Incoming Messages

    pub fn read_messages(
        &mut self,
        host_tick: &Tick,
        remote_tick: &Tick,
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
                channel.read_messages(host_tick, remote_tick, channel_reader, bit_reader);
            }
        }
    }

    pub fn receive_messages(&mut self, host_tick: &Tick) -> Vec<(C, P)> {
        let mut output = Vec::new();
        for (channel_index, channel) in &mut self.channel_receivers {
            let mut messages = channel.receive_messages(host_tick);
            for message in messages.drain(..) {
                output.push((channel_index.clone(), message));
            }
        }
        return output;
    }
}

impl<P: Protocolize, C: ChannelIndex> PacketNotifiable for TickBuffer<P, C> {
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(channel_list) = self.packet_to_channel_map.get(&packet_index) {
            for (channel_index, message_ids) in channel_list {
                if let Some(channel) = self.channel_senders.get_mut(channel_index) {
                    for (tick, message_id) in message_ids {
                        channel.notify_message_delivered(tick, message_id);
                    }
                }
            }
        }
    }
}
