use std::{
    collections::VecDeque,
    time::Duration
};

use naia_socket_shared::Instant;
use crate::{sequence_greater_than, sequence_less_than};

use super::{
    protocolize::Protocolize, types::MessageId,
    ChannelIndex, ReliableSettings
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct ReliableMessageManager<P: Protocolize> {
    ordered: bool,
    rtt_resend_factor: f32,
    outgoing_message_id: MessageId,
    outgoing_messages: VecDeque<Option<(MessageId, Option<Instant>, P)>>,
    incoming_message_id: MessageId,
    incoming_messages: VecDeque<(MessageId, Option<P>)>
}

impl<P: Protocolize> ReliableMessageManager<P> {
    /// Creates a new MessageManager
    pub fn new(reliable_settings: &ReliableSettings, ordered: bool) -> Self {
        let mut incoming_messages = VecDeque::new();

        ReliableMessageManager {
            ordered,
            rtt_resend_factor: reliable_settings.rtt_resend_factor,
            incoming_message_id: 0,
            incoming_messages,
            outgoing_message_id: 0,
            outgoing_messages: VecDeque::new(),
        }
    }

    pub fn recv_message(&mut self, message_id: MessageId, message: P) {

        // moving from oldest incoming message to newest
        // compare existing slots and see if the message_id has been instantiated already
        // if it has, put the message into the slot
        // otherwise, keep track of what the last message id was
        // then add new empty slots at the end until getting to the incoming message id
        // then, once you're there, put the new message in

        if sequence_less_than(message_id, self.incoming_message_id) {
            // already moved sliding window past this message id
            return;
        }

        let mut index = 0;
        let mut found = false;

        loop {
            if index < self.incoming_messages.len() {
                if let Some((old_message_id, _)) = self.incoming_messages.get(index) {
                    if *old_message_id == message_id {
                        found = true;
                    }
                }

                if found {
                    let (_, old_message) = self.incoming_messages.get_mut(index).unwrap();
                    if old_message.is_none() {
                        *old_message = Some(message);
                        break;
                    } else {
                        // already received this message
                    }
                }
            } else {
                let next_message_id = self.incoming_message_id.wrapping_add(index as u16);

                if next_message_id == message_id {
                    self.incoming_messages.push_back((next_message_id, Some(message)));
                    break;
                } else {
                    self.incoming_messages.push_back((next_message_id, None));
                }
            }

            index += 1;
        }
    }

    pub fn generate_incoming_messages<C: ChannelIndex>(&mut self,
                                      channel_index: &C,
                                      incoming_messages: &mut VecDeque<(C, P)>) {
        loop {
            let mut has_message = false;
            if let Some((_, Some(_))) = self.incoming_messages.front() {
                has_message = true;
            }
            if has_message {
                let (_, message_opt) = self.incoming_messages.pop_front().unwrap();
                let message = message_opt.unwrap();
                incoming_messages.push_back((channel_index.clone(), message));
                self.incoming_message_id = self.incoming_message_id.wrapping_add(1);
            } else {
                break;
            }
        }
    }

    pub fn send_message(&mut self, message: P) {
        self.outgoing_messages.push_back(Some((self.outgoing_message_id, None, message)));
        self.outgoing_message_id = self.outgoing_message_id.wrapping_add(1);
    }

    pub fn generate_outgoing_messages<C: ChannelIndex>(&mut self,
                                                       rtt_millis: &f32,
                                                       channel_index: &C,
                                                       outgoing_messages: &mut VecDeque<(C, MessageId, P)>) {
        let resend_duration = Duration::from_millis((self.rtt_resend_factor * rtt_millis) as u64);
        let now = Instant::now();

        for message_opt in self.outgoing_messages.iter_mut() {
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
            if index == self.outgoing_messages.len() {
                break;
            }

            if let Some(Some((old_message_id, _, _))) = self.outgoing_messages.get(index) {
                if *message_id == *old_message_id {
                    found = true;
                }
            }

            if found {
                // replace found message with nothing
                let container = self.outgoing_messages.get_mut(index).unwrap();
                *container = None;

                // keep popping off Nones from the front of the Vec
                loop {
                    let mut pop = false;
                    if let Some(message_opt) = self.outgoing_messages.front() {
                        if message_opt.is_none() {
                            pop = true;
                        }
                    }
                    if pop {
                        self.outgoing_messages.pop_front();
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