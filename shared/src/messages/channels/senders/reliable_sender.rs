use std::{collections::VecDeque, mem, time::Duration};

use naia_serde::BitWriter;
use naia_socket_shared::Instant;

use crate::{
    messages::{
        channels::senders::{
            channel_sender::{ChannelSender, MessageChannelSender},
            indexed_message_writer::IndexedMessageWriter,
        },
        message_container::MessageContainer,
        message_kinds::MessageKinds,
    },
    types::MessageIndex,
    LocalEntityAndGlobalEntityConverterMut,
};

// Sender
pub struct ReliableSender<P: Send + Sync> {
    rtt_resend_factor: f32,
    sending_messages: VecDeque<Option<(MessageIndex, Option<Instant>, P)>>,
    next_send_message_index: MessageIndex,
    outgoing_messages: VecDeque<(MessageIndex, P)>,
}

impl<P: Send + Sync> ReliableSender<P> {
    pub fn new(rtt_resend_factor: f32) -> Self {
        Self {
            rtt_resend_factor,
            next_send_message_index: 0,
            sending_messages: VecDeque::new(),
            outgoing_messages: VecDeque::new(),
        }
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

    pub fn take_next_messages(&mut self) -> VecDeque<(MessageIndex, P)> {
        mem::take(&mut self.outgoing_messages)
    }

    // Called when a message has been delivered
    // If this message has never been delivered before, will clear from the outgoing
    // buffer and return the message previously there
    pub fn deliver_message(&mut self, message_index: &MessageIndex) -> Option<P> {
        let mut index = 0;
        let mut found = false;

        loop {
            if index == self.sending_messages.len() {
                return None;
            }

            if let Some(Some((old_message_index, _, _))) = self.sending_messages.get(index) {
                if *message_index == *old_message_index {
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

impl<P: Send + Sync + Clone> ChannelSender<P> for ReliableSender<P> {
    fn send_message(&mut self, message: P) {
        self.sending_messages
            .push_back(Some((self.next_send_message_index, None, message)));
        self.next_send_message_index = self.next_send_message_index.wrapping_add(1);
    }

    fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        let resend_duration = Duration::from_millis((self.rtt_resend_factor * rtt_millis) as u64);

        for (message_index, last_sent_opt, message) in self.sending_messages.iter_mut().flatten() {
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
                    .push_back((*message_index, message.clone()));
                *last_sent_opt = Some(now.clone());
            }
        }
    }

    fn has_messages(&self) -> bool {
        !self.outgoing_messages.is_empty()
    }

    fn notify_message_delivered(&mut self, message_index: &MessageIndex) {
        self.deliver_message(message_index);
    }
}

impl MessageChannelSender for ReliableSender<MessageContainer> {
    fn write_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>> {
        IndexedMessageWriter::write_messages(
            message_kinds,
            &mut self.outgoing_messages,
            converter,
            writer,
            has_written,
        )
    }
}
