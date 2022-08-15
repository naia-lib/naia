use std::{collections::VecDeque, mem, time::Duration};

use naia_serde::{BitCounter, BitWrite, BitWriter, Serde, UnsignedVariableInteger};

use naia_socket_shared::Instant;

use crate::{constants::MTU_SIZE_BITS, types::MessageId, wrapping_diff};

use super::{
    message_channel::{ChannelSender, ChannelWriter},
    message_list_header,
};

// Sender

pub struct ReliableSender<P: Send + Sync> {
    rtt_resend_factor: f32,
    sending_messages: VecDeque<Option<(MessageId, Option<Instant>, P)>>,
    next_send_message_id: MessageId,
    next_send_messages: VecDeque<(MessageId, P)>,
}

impl<P: Send + Sync> ReliableSender<P> {
    pub fn new(rtt_resend_factor: f32) -> Self {
        Self {
            rtt_resend_factor,
            next_send_message_id: 0,
            sending_messages: VecDeque::new(),
            next_send_messages: VecDeque::new(),
        }
    }

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

    pub fn cleanup_sent_messages(&mut self) {
        // keep popping off Nones from the front of the Vec
        loop {
            let mut pop = false;
            if let Some(message_opt) = self.sending_messages.front() {
                if message_opt.is_none() {
                    pop = true;
                }
            }
            if pop {
                self.sending_messages.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn take_next_messages(&mut self) -> VecDeque<(MessageId, P)> {
        mem::take(&mut self.next_send_messages)
    }

    // Called when a message has been delivered
    // If this message has never been delivered before, will clear from the outgoing
    // buffer and return the message previously there
    pub fn deliver_message(&mut self, message_id: &MessageId) -> Option<P> {
        let mut index = 0;
        let mut found = false;

        loop {
            if index == self.sending_messages.len() {
                return None;
            }

            if let Some(Some((old_message_id, _, _))) = self.sending_messages.get(index) {
                if *message_id == *old_message_id {
                    found = true;
                }
            }

            if found {
                // replace found message with nothing
                let container = self.sending_messages.get_mut(index).unwrap();
                let output = container.take();

                self.cleanup_sent_messages();

                // stop loop
                return output.map(|(_, _, message)| message);
            }

            index += 1;
        }
    }
}

impl<P: Clone + Send + Sync> ChannelSender<P> for ReliableSender<P> {
    fn send_message(&mut self, message: P) {
        self.sending_messages
            .push_back(Some((self.next_send_message_id, None, message)));
        self.next_send_message_id = self.next_send_message_id.wrapping_add(1);
    }

    fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        let resend_duration = Duration::from_millis((self.rtt_resend_factor * rtt_millis) as u64);

        for (message_id, last_sent_opt, message) in self.sending_messages.iter_mut().flatten() {
            let mut should_send = false;
            if let Some(last_sent) = last_sent_opt {
                if last_sent.elapsed() >= resend_duration {
                    should_send = true;
                }
            } else {
                should_send = true;
            }
            if should_send {
                self.next_send_messages
                    .push_back((*message_id, message.clone()));
                *last_sent_opt = Some(now.clone());
            }
        }
    }

    fn has_messages(&self) -> bool {
        !self.next_send_messages.is_empty()
    }

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
                message_list_header::write(bit_writer, 0);
                return None;
            }

            let mut counter = BitCounter::new();

            //TODO: message_count is inaccurate here and may be different than final, does
            // this matter?
            message_list_header::write(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                message_list_header::write(bit_writer, 0);
                return None;
            }

            // Find how many messages will fit into the packet
            let mut last_written_id: Option<MessageId> = None;
            let mut index = 0;
            loop {
                if index >= self.next_send_messages.len() {
                    break;
                }

                let (message_id, message) = self.next_send_messages.get(index).unwrap();
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
        message_list_header::write(bit_writer, message_count);

        // Messages
        {
            let mut last_written_id: Option<MessageId> = None;
            let mut message_ids = Vec::new();

            for _ in 0..message_count {
                // Pop and write message
                let (message_id, message) = self.next_send_messages.pop_front().unwrap();
                self.write_outgoing_message(
                    channel_writer,
                    bit_writer,
                    &last_written_id,
                    &message_id,
                    &message,
                );

                message_ids.push(message_id);
                last_written_id = Some(message_id);
            }
            Some(message_ids)
        }
    }

    fn notify_message_delivered(&mut self, message_id: &MessageId) {
        self.deliver_message(message_id);
    }
}
