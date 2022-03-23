use std::collections::{HashMap, VecDeque};

use crate::types::MsgId;

use naia_shared::{serde::{BitCounter, BitWrite, BitWriter, Serde}, write_list_header, ChannelIndex, NetEntityHandleConverter, PacketIndex, PacketNotifiable, Protocolize, ReplicateSafe, Tick, MTU_SIZE_BITS, ChannelConfig};
use crate::channel_tick_buffer::ChannelTickBuffer;

pub struct TickBuffer<P: Protocolize, C: ChannelIndex> {
    channels: HashMap<C, ChannelTickBuffer<P>>,
    outgoing_messages: VecDeque<(MsgId, Tick, C, P)>,
    packet_to_channel_map: HashMap<PacketIndex, C>,
}

impl<P: Protocolize, C: ChannelIndex> TickBuffer<P, C> {
    pub fn new(channel_config: &ChannelConfig<C>,) -> Self {

        // initialize all tick buffer channels
        let mut channels = HashMap::new();
        let all_channel_settings = channel_config.all_tick_buffer_settings();
        for (index, settings) in all_channel_settings {
            let new_channel = ChannelTickBuffer::new(&settings);
            channels.insert(index, new_channel);
        }

        TickBuffer {
            channels,
            outgoing_messages: VecDeque::new(),
            packet_to_channel_map: HashMap::new(),
        }
    }

    pub fn generate_resend_messages(&mut self, server_receivable_tick: &Tick) {
        for (channel_index, channel) in &mut self.channels {
            channel.generate_resend_messages(server_receivable_tick, channel_index, &mut self.outgoing_messages);
        }
    }

    pub fn send_message<R: ReplicateSafe<P>>(
        &mut self,
        client_tick: Tick,
        channel_index: C,
        message: &R,
    ) {
        if let Some(channel) = self.channels.get_mut(&channel_index) {
            channel.send_message(client_tick, message);
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        return self.outgoing_messages.len() > 0;
    }

    // Tick Buffer Message Writing

    pub fn write_messages(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
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
            for (message_id, client_tick, channel, message) in self.outgoing_messages.iter() {
                self.write_message(
                    converter,
                    &mut counter,
                    &client_tick,
                    &message_id,
                    channel,
                    message,
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
                let (message_id, client_tick, channel, message) =
                    self.outgoing_messages.pop_front().unwrap();

                // Write message
                self.write_message(
                    converter,
                    writer,
                    &client_tick,
                    &message_id,
                    &channel,
                    &message,
                );
                if let Some(channel_buffer) = self.channels.get_mut(&channel) {
                    channel_buffer.message_written(packet_index, client_tick, message_id);
                }
                self.packet_to_channel_map.insert(packet_index, channel);
            }
        }
    }

    /// Writes a Command into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_message<S: BitWrite>(
        &self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut S,
        client_tick: &Tick,
        message_id: &MsgId,
        channel: &C,
        message: &P,
    ) {
        // write client tick
        client_tick.ser(writer);

        // write message id
        let short_msg_id: u8 = (message_id % 256) as u8;
        short_msg_id.ser(writer);

        // write message channel
        channel.ser(writer);

        // write message kind
        message.dyn_ref().kind().ser(writer);

        // write payload
        message.write(writer, converter);
    }
}

impl<P: Protocolize, C: ChannelIndex> PacketNotifiable for TickBuffer<P, C> {
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(channel_index) = self.packet_to_channel_map.get(&packet_index) {
            if let Some(channel) = self.channels.get_mut(channel_index) {
                channel.notify_packet_delivered(packet_index);
            }
        }
    }

    fn notify_packet_dropped(&mut self, _dropped_packet_index: PacketIndex) {}
}