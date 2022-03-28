use std::collections::HashMap;

use naia_serde::BitReader;

use super::{
    channel_config::{ChannelConfig, ChannelIndex, ChannelMode},
    channel_tick_buffer::ChannelTickBuffer,
};

use crate::{
    connection::packet_notifiable::PacketNotifiable,
    protocol::{
        entity_property::NetEntityHandleConverter, protocolize::Protocolize,
        replicate::ReplicateSafe,
    },
    serde::BitWriter,
    types::{PacketIndex, ShortMessageId, Tick},
    vecmap::VecMap,
    ChannelWriter,
};

pub struct TickBuffer<P: Protocolize, C: ChannelIndex> {
    channels: VecMap<C, ChannelTickBuffer<P>>,
    packet_to_channel_map: HashMap<PacketIndex, Vec<(C, Vec<(Tick, ShortMessageId)>)>>,
}

impl<P: Protocolize, C: ChannelIndex> TickBuffer<P, C> {
    pub fn new(channel_config: &ChannelConfig<C>) -> Self {
        // initialize all tick buffer channels
        let mut channels = VecMap::new();

        for channel_index in &channel_config.channels().vec {
            let channel = channel_config.channels().map.get(channel_index).unwrap();
            match &channel.mode {
                ChannelMode::TickBuffered(settings) => {
                    let new_channel = ChannelTickBuffer::new(&settings);
                    channels.dual_insert(channel_index.clone(), new_channel);
                }
                _ => {}
            }
        }

        TickBuffer {
            channels,
            packet_to_channel_map: HashMap::new(),
        }
    }

    pub fn collect_incoming_messages(&mut self, host_tick: &Tick) -> Vec<(C, P)> {
        let mut output = Vec::new();
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get_mut(channel_index).unwrap();
            let mut messages = channel.collect_incoming_messages(host_tick);
            for message in messages.drain(..) {
                output.push((channel_index.clone(), message));
            }
        }
        return output;
    }

    pub fn collect_outgoing_messages(
        &mut self,
        client_sending_tick: &Tick,
        server_receivable_tick: &Tick,
    ) {
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get_mut(channel_index).unwrap();
            channel.collect_outgoing_messages(client_sending_tick, server_receivable_tick);
        }
    }

    pub fn send_message<R: ReplicateSafe<P>>(
        &mut self,
        host_tick: &Tick,
        channel_index: C,
        message: &R,
    ) {
        if let Some(channel) = self.channels.map.get_mut(&channel_index) {
            channel.send_message(host_tick, message);
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get(channel_index).unwrap();
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
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get_mut(channel_index).unwrap();
            if let Some(message_ids) = channel.write_messages(channel_writer, bit_writer, host_tick)
            {
                // {
                //     let mut messages_string = "".to_string();
                //     for (tick, message_id) in &message_ids {
                //         messages_string += &format!("(t{}, i{})", tick, message_id);
                //     }
                //     info!("Writing Packet ({}), with messages: [{}]", packet_index,
                // messages_string); }

                if !self.packet_to_channel_map.contains_key(&packet_index) {
                    self.packet_to_channel_map.insert(packet_index, Vec::new());
                }
                let channel_list = self.packet_to_channel_map.get_mut(&packet_index).unwrap();
                channel_list.push((channel_index.clone(), message_ids));
            }
        }
    }

    pub fn read_messages(
        &mut self,
        host_tick: &Tick,
        remote_tick: &Tick,
        reader: &mut BitReader,
        converter: &mut dyn NetEntityHandleConverter,
    ) {
        for channel_index in &self.channels.vec {
            let channel = self.channels.map.get_mut(channel_index).unwrap();
            channel.read_messages(host_tick, remote_tick, reader, converter);
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> PacketNotifiable for TickBuffer<P, C> {
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(channel_list) = self.packet_to_channel_map.get(&packet_index) {
            for (channel_index, message_ids) in channel_list {
                if let Some(channel) = self.channels.map.get_mut(channel_index) {
                    for (tick, message_id) in message_ids {
                        channel.notify_message_delivered(tick, message_id);
                    }
                }
            }
        }
    }

    fn notify_packet_dropped(&mut self, _dropped_packet_index: PacketIndex) {}
}
