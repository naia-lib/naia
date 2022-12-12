use std::collections::VecDeque;

use naia_serde::{BitCounter, BitWrite, BitWriter, Serde, UnsignedVariableInteger};
use naia_socket_shared::Instant;

use crate::{constants::MTU_SIZE_BITS, types::MessageId, wrapping_diff};

use super::{
    message_channel::{ChannelSender, ChannelWriter},
    message_list_header::write,
};

pub struct SequencedUnreliableSender<P: Send> {
    /// Buffer of the next messages to send along with their MessageId
    outgoing_messages: VecDeque<(MessageId, P)>,
    /// Next message id to use (not yet used in the buffer)
    next_send_message_id: MessageId,
}

impl<P: Send> SequencedUnreliableSender<P> {
    pub fn new() -> Self {
        Self {
            outgoing_messages: VecDeque::new(),
            next_send_message_id: 0,
        }
    }

    /// Write a message in the channel_writer. Will include the wrapped message id and the message
    /// data
    fn write_outgoing_message(
        &self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut dyn BitWrite,
        last_written_id: &Option<MessageId>,
        message_id: &MessageId,
        message: &P,
    ) {
        if let Some(last_id) = last_written_id {
            // write message id diff
            let id_diff = wrapping_diff(*last_id, *message_id);
            let id_diff_encoded = UnsignedVariableInteger::<3>::new(id_diff);
            id_diff_encoded.ser(bit_writer);
        } else {
            // write message id
            message_id.ser(bit_writer);
        }

        channel_writer.write(bit_writer, message);
    }
}

impl<P: Send + Sync> ChannelSender<P> for SequencedUnreliableSender<P> {
    fn send_message(&mut self, message: P) {
        self.outgoing_messages.push_back((self.next_send_message_id, message));
        self.next_send_message_id = self.next_send_message_id.wrapping_add(1);
    }

    fn collect_messages(&mut self, _: &Instant, _: &f32) {
        // not necessary for an unreliable channel
    }

    fn has_messages(&self) -> bool {
        !self.outgoing_messages.is_empty()
    }

    /// Write messages from the buffer into the channel
    /// Include a wrapped message id for sequencing purposes
    fn write_messages(
        &mut self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut BitWriter,
    ) -> Option<Vec<MessageId>> {
        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = bit_writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                write(bit_writer, 0);
                return None;
            }

            let mut counter = BitCounter::new();

            //TODO: message_count is inaccurate here and may be different than final, does
            // this matter?
            write(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                write(bit_writer, 0);
                return None;
            }

            // Find how many messages will fit into the packet
            let mut last_written_id: Option<MessageId> = None;
            let mut index = 0;
            loop {
                if index >= self.outgoing_messages.len() {
                    break;
                }

                let (message_id, message) = self.outgoing_messages.get(index).unwrap();
                self.write_outgoing_message(
                    channel_writer,
                    &mut counter,
                    &last_written_id,
                    message_id,
                    message,
                );
                last_written_id = Some(*message_id);
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                } else {
                    break;
                }

                index += 1;
            }
        }

        // Write header
        write(bit_writer, message_count);

        // Messages
        {
            let mut last_written_id: Option<MessageId> = None;
            for _ in 0..message_count {
                // Pop and write message
                let (message_id, message) = self.outgoing_messages.pop_front().unwrap();
                self.write_outgoing_message(
                    channel_writer,
                    bit_writer,
                    &last_written_id,
                    &message_id,
                    &message,
                );
                last_written_id = Some(message_id);
            }
            None
        }
    }

    fn notify_message_delivered(&mut self, _: &MessageId) {
        // not necessary for an unreliable channel
    }
}
