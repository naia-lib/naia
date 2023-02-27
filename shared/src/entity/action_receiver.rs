use std::{collections::VecDeque, hash::Hash};

use crate::{sequence_less_than, EntityAction, MessageIndex};

pub struct ActionReceiver<E: Copy + Hash + Eq> {
    oldest_received_message_index: MessageIndex,
    record: VecDeque<(MessageIndex, bool)>,
    incoming_messages: Vec<(MessageIndex, EntityAction<E>)>,
}

// TODO: much of this is copied directly from reliable_receiver.rs ... need to refactor

impl<E: Copy + Hash + Eq> ActionReceiver<E> {
    pub fn new() -> Self {
        Self {
            oldest_received_message_index: 0,
            record: VecDeque::default(),
            incoming_messages: Vec::default(),
        }
    }

    pub(crate) fn buffer_message(&mut self, message_index: MessageIndex, message: EntityAction<E>) {
        // moving from oldest incoming message to newest
        // compare existing slots and see if the message_index has been instantiated
        // already if it has, put the message into the slot
        // otherwise, keep track of what the last message id was
        // then add new empty slots at the end until getting to the incoming message id
        // then, once you're there, put the new message in

        if sequence_less_than(message_index, self.oldest_received_message_index) {
            // already moved sliding window past this message id
            return;
        }

        let mut current_index = 0;

        loop {
            let mut should_push_message = false;
            if current_index < self.record.len() {
                if let Some((old_message_index, old_message)) = self.record.get_mut(current_index) {
                    if *old_message_index == message_index {
                        if !(*old_message) {
                            *old_message = true;
                            should_push_message = true;
                        } else {
                            // already received this message
                            return;
                        }
                    }
                }
            } else {
                let next_message_index = self
                    .oldest_received_message_index
                    .wrapping_add(current_index as u16);

                if next_message_index == message_index {
                    self.record.push_back((next_message_index, true));
                    should_push_message = true;
                } else {
                    self.record.push_back((next_message_index, false));
                    // keep filling up buffer
                }
            }

            if should_push_message {
                self.incoming_messages.push((message_index, message));
                self.clear_old_messages();
                return;
            }

            current_index += 1;
        }
    }

    fn clear_old_messages(&mut self) {
        // clear all received messages from record
        loop {
            let mut has_message = false;
            if let Some((_, true)) = self.record.front() {
                has_message = true;
            }
            if has_message {
                self.record.pop_front();
                self.oldest_received_message_index =
                    self.oldest_received_message_index.wrapping_add(1);
            } else {
                break;
            }
        }
    }

    pub(crate) fn receive_messages(&mut self) -> Vec<(MessageIndex, EntityAction<E>)> {
        std::mem::take(&mut self.incoming_messages)
    }
}
