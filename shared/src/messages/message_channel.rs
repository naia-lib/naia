use std::{collections::VecDeque, time::Duration};

use naia_socket_shared::Instant;

use crate::{protocol::protocolize::Protocolize, types::MessageId, ChannelIndex};

use super::channel_config::ReliableSettings;

pub trait MessageChannel<P: Protocolize, C: ChannelIndex> {
    fn send_message(&mut self, message: P);
    fn recv_message(&mut self, message_id: MessageId, message: P);
    fn collect_outgoing_messages(
        &mut self,
        rtt_millis: &f32,
        outgoing_messages: &mut VecDeque<(C, MessageId, P)>,
    );
    fn collect_incoming_messages(&mut self, incoming_messages: &mut Vec<(C, P)>);
    fn notify_message_delivered(&mut self, message_id: &MessageId);
}

pub struct OutgoingReliableChannel<P: Protocolize, C: ChannelIndex> {
    channel_index: C,
    rtt_resend_factor: f32,
    outgoing_message_id: MessageId,
    outgoing_message_buffer: VecDeque<Option<(MessageId, Option<Instant>, P)>>,
}

impl<P: Protocolize, C: ChannelIndex> OutgoingReliableChannel<P, C> {
    pub fn new(channel_index: C, reliable_settings: &ReliableSettings) -> Self {
        Self {
            channel_index,
            rtt_resend_factor: reliable_settings.rtt_resend_factor,
            outgoing_message_id: 0,
            outgoing_message_buffer: VecDeque::new(),
        }
    }

    pub fn send_message(&mut self, message: P) {
        self.outgoing_message_buffer
            .push_back(Some((self.outgoing_message_id, None, message)));
        self.outgoing_message_id = self.outgoing_message_id.wrapping_add(1);
    }

    pub fn generate_messages(
        &mut self,
        rtt_millis: &f32,
        outgoing_messages: &mut VecDeque<(C, MessageId, P)>,
    ) {
        let resend_duration = Duration::from_millis((self.rtt_resend_factor * rtt_millis) as u64);
        let now = Instant::now();

        for message_opt in self.outgoing_message_buffer.iter_mut() {
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
                    outgoing_messages.push_back((
                        self.channel_index.clone(),
                        *message_id,
                        message.clone(),
                    ));
                    *last_sent_opt = Some(now.clone());
                }
            }
        }
    }

    pub fn notify_message_delivered(&mut self, message_id: &MessageId) {
        let mut index = 0;
        let mut found = false;

        loop {
            if index == self.outgoing_message_buffer.len() {
                break;
            }

            if let Some(Some((old_message_id, _, _))) = self.outgoing_message_buffer.get(index) {
                if *message_id == *old_message_id {
                    found = true;
                }
            }

            if found {
                // replace found message with nothing
                let container = self.outgoing_message_buffer.get_mut(index).unwrap();
                *container = None;

                // keep popping off Nones from the front of the Vec
                loop {
                    let mut pop = false;
                    if let Some(message_opt) = self.outgoing_message_buffer.front() {
                        if message_opt.is_none() {
                            pop = true;
                        }
                    }
                    if pop {
                        self.outgoing_message_buffer.pop_front();
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
