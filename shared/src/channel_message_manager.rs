use std::{
    collections::VecDeque,
    time::Duration
};

use naia_socket_shared::Instant;

use super::{
    protocolize::Protocolize, types::MessageId,
    ChannelIndex, ReliableSettings
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct ReliableMessageManager<P: Protocolize> {
    messages: VecDeque<Option<(MessageId, Option<Instant>, P)>>,
    rtt_resend_factor: f32,
    current_message_id: MessageId,
}

impl<P: Protocolize> ReliableMessageManager<P> {
    /// Creates a new MessageManager
    pub fn new(reliable_settings: &ReliableSettings) -> Self {
        ReliableMessageManager {
            rtt_resend_factor: reliable_settings.rtt_resend_factor,
            messages: VecDeque::new(),
            current_message_id: 0,
        }
    }

    pub fn send_message(&mut self, message: P) {
        self.messages.push_back(Some((self.current_message_id, None, message)));
        self.current_message_id = self.current_message_id.wrapping_add(1);
    }

    pub fn generate_resend_messages<C: ChannelIndex>(&mut self,
                                                     rtt_millis: &f32,
                                                     channel_index: &C,
                                                     outgoing_messages: &mut VecDeque<(C, MessageId, P)>) {
        let resend_duration = Duration::from_millis((self.rtt_resend_factor * rtt_millis) as u64);
        let now = Instant::now();

        for message_opt in self.messages.iter_mut() {
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
                    outgoing_messages.push_back((channel_index.clone(), *message_id, message.clone()));
                    *last_sent_opt = Some(now.clone());
                }
            }
        }
    }

    pub fn notify_message_delivered(&mut self, message_id: &MessageId) {
        let mut index = 0;
        let mut found = false;

        loop {
            if index == self.messages.len() {
                break;
            }

            if let Some(Some((old_message_id, _, _))) = self.messages.get(index) {
                if *message_id == *old_message_id {
                    found = true;
                }
            }

            if found {
                // replace found message with nothing
                let container = self.messages.get_mut(index).unwrap();
                *container = None;

                // keep popping off Nones from the front of the Vec
                while self.messages.front().unwrap().is_none() {
                    self.messages.pop_front();
                }

                // stop loop
                break;
            }

            index += 1;
        }
    }
}