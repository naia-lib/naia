use std::{collections::VecDeque, mem, time::Duration};
use log::warn;

use naia_serde::{BitWrite, BitWriter, Serde, UnsignedVariableInteger};

use naia_socket_shared::Instant;

use crate::{types::MessageId, wrapping_diff};

use super::message_channel::{ChannelSender, ChannelWriter};

// Sender

pub struct ReliableSender<P: Send + Sync> {
    rtt_resend_factor: f32,
    sending_messages: VecDeque<Option<(MessageId, Option<Instant>, P)>>,
    next_send_message_id: MessageId,
    outgoing_messages: VecDeque<(MessageId, P)>,
}

impl<P: Send + Sync> ReliableSender<P> {
    pub fn new(rtt_resend_factor: f32) -> Self {
        Self {
            rtt_resend_factor,
            next_send_message_id: 0,
            sending_messages: VecDeque::new(),
            outgoing_messages: VecDeque::new(),
        }
    }

    fn write_message(
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

    fn warn_overflow(&self, bits_needed: u16, bits_free: u16) {
        warn!(
            "Packet Write Error: Blocking overflow detected! Message requires {bits_needed} bits, but packet only has {bits_free} bits available! This condition should never be reached, as large Messages should be Fragmented in the Reliable channel"
        )
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
        mem::take(&mut self.outgoing_messages)
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
                self.outgoing_messages
                    .push_back((*message_id, message.clone()));
                *last_sent_opt = Some(now.clone());
            }
        }
    }

    fn has_messages(&self) -> bool {
        !self.outgoing_messages.is_empty()
    }

    fn write_messages(
        &mut self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageId>> {
        let mut last_written_id: Option<MessageId> = None;
        let mut message_ids = Vec::new();

        loop {
            if self.outgoing_messages.is_empty() {
                break;
            }

            // check that we can write the next message
            let (message_id, message) = self.outgoing_messages.front().unwrap();
            let mut counter = bit_writer.counter();
            self.write_message(
                channel_writer,
                &mut counter,
                &last_written_id,
                message_id,
                message,
            );

            if counter.overflowed() {
                // if nothing useful has been written in this packet yet,
                // send warning about size of message being too big
                if !*has_written {
                    self.warn_overflow(counter.bits_needed(), bit_writer.bits_free());
                }

                break;
            }

            *has_written = true;

            // write MessageContinue bit
            true.ser(bit_writer);

            // write data
            self.write_message(
                channel_writer,
                bit_writer,
                &last_written_id,
                message_id,
                message,
            );

            message_ids.push(*message_id);
            last_written_id = Some(*message_id);

            // pop message we've written
            self.outgoing_messages.pop_front();
        }
        Some(message_ids)
    }

    fn notify_message_delivered(&mut self, message_id: &MessageId) {
        self.deliver_message(message_id);
    }
}
