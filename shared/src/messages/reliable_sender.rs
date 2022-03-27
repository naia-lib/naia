use std::{collections::VecDeque, time::Duration};

use naia_serde::{BitCounter, BitWrite, BitWriter, Serde, UnsignedVariableInteger};

use naia_socket_shared::Instant;

use crate::{
    constants::MTU_SIZE_BITS,
    messages::message_list_header,
    protocol::{entity_property::NetEntityHandleConverter, protocolize::Protocolize},
    types::MessageId,
    wrapping_diff,
};

use super::{channel_config::ReliableSettings, message_channel::ChannelSender};

// Sender

pub struct ReliableSender<P: Protocolize> {
    rtt_resend_factor: f32,
    next_send_message_id: MessageId,
    sending_messages: VecDeque<Option<(MessageId, Option<Instant>, P)>>,
    next_send_messages: VecDeque<(MessageId, P)>,
}

impl<P: Protocolize> ReliableSender<P> {
    pub fn new(reliable_settings: &ReliableSettings) -> Self {
        Self {
            rtt_resend_factor: reliable_settings.rtt_resend_factor,
            next_send_message_id: 0,
            sending_messages: VecDeque::new(),
            next_send_messages: VecDeque::new(),
        }
    }

    fn write_outgoing_message<S: BitWrite>(
        &self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut S,
        last_written_id: &Option<MessageId>,
        message_id: &MessageId,
        message: &P,
    ) {
        if let Some(last_id) = last_written_id {
            // write message id diff
            let id_diff = wrapping_diff(*last_id, *message_id);
            let id_diff_encoded = UnsignedVariableInteger::<3>::new(id_diff);
            id_diff_encoded.ser(writer);
        } else {
            // write message id
            message_id.ser(writer);
        }

        // write message kind
        message.dyn_ref().kind().ser(writer);

        // write payload
        message.write(writer, converter);
    }
}

impl<P: Protocolize> ChannelSender<P> for ReliableSender<P> {
    fn send_message(&mut self, message: P) {
        self.sending_messages
            .push_back(Some((self.next_send_message_id, None, message)));
        self.next_send_message_id = self.next_send_message_id.wrapping_add(1);
    }

    fn collect_messages(&mut self, rtt_millis: &f32) {
        let resend_duration = Duration::from_millis((self.rtt_resend_factor * rtt_millis) as u64);
        let now = Instant::now();

        for message_opt in self.sending_messages.iter_mut() {
            if let Some((message_id, last_sent_opt, message)) = message_opt {
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
    }

    fn has_messages(&self) -> bool {
        return self.next_send_messages.len() != 0;
    }

    fn write_messages(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
    ) -> Option<Vec<MessageId>> {
        let mut message_count: u16 = 0;

        // Header
        {
            // Measure
            let current_packet_size = writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                message_list_header::write(writer, 0);
                return None;
            }

            let mut counter = BitCounter::new();

            //TODO: message_count is inaccurate here and may be different than final, does
            // this matter?
            message_list_header::write(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                message_list_header::write(writer, 0);
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
                    converter,
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
        message_list_header::write(writer, message_count);

        // Messages
        {
            let mut last_written_id: Option<MessageId> = None;
            let mut message_ids = Vec::new();

            for _ in 0..message_count {
                // Pop and write message
                let (message_id, message) = self.next_send_messages.pop_front().unwrap();
                self.write_outgoing_message(
                    converter,
                    writer,
                    &last_written_id,
                    &message_id,
                    &message,
                );

                message_ids.push(message_id);
                last_written_id = Some(message_id);
            }
            return Some(message_ids);
        }
    }

    fn notify_message_delivered(&mut self, message_id: &MessageId) {
        let mut index = 0;
        let mut found = false;

        loop {
            if index == self.sending_messages.len() {
                break;
            }

            if let Some(Some((old_message_id, _, _))) = self.sending_messages.get(index) {
                if *message_id == *old_message_id {
                    found = true;
                }
            }

            if found {
                // replace found message with nothing
                let container = self.sending_messages.get_mut(index).unwrap();
                *container = None;

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

                // stop loop
                break;
            }

            index += 1;
        }
    }
}
