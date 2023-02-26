use std::collections::VecDeque;

use crate::{
    messages::channels::reliable_receiver::{ReceiverArranger, ReliableReceiver},
    types::MessageIndex,
};

// OrderedReliableReceiver
pub type OrderedReliableReceiver<M> = ReliableReceiver<OrderedArranger<M>, M>;

impl<M: Send + Sync> OrderedReliableReceiver<M> {
    pub fn new() -> Self {
        Self::with_arranger(OrderedArranger::<M> {
            oldest_received_message_index: 0,
            buffer: VecDeque::new(),
        })
    }
}

// OrderedArranger
pub struct OrderedArranger<M> {
    buffer: VecDeque<(MessageIndex, Option<M>)>,
    oldest_received_message_index: MessageIndex,
}

impl<M: Send + Sync> ReceiverArranger<M> for OrderedArranger<M> {
    fn process(
        &mut self,
        incoming_messages: &mut Vec<(MessageIndex, M)>,
        message_index: MessageIndex,
        message: M,
    ) {
        let mut current_index = 0;

        // Put message where it needs to go in buffer
        loop {
            if current_index < self.buffer.len() {
                if let Some((old_message_index, old_message)) = self.buffer.get_mut(current_index) {
                    if *old_message_index == message_index {
                        if old_message.is_none() {
                            *old_message = Some(message);
                            break;
                        }
                    }
                }
            } else {
                let next_message_index = self
                    .oldest_received_message_index
                    .wrapping_add(current_index as u16);

                if next_message_index == message_index {
                    self.buffer.push_back((next_message_index, Some(message)));
                    break;
                } else {
                    self.buffer.push_back((next_message_index, None));
                    // keep filling up buffer
                }
            }

            current_index += 1;
        }

        // Pop messages out in order
        loop {
            let Some((_, Some(_))) = self.buffer.front() else {
                // no more messages, return
                return;
            };
            let Some((index, Some(message))) = self.buffer.pop_front() else {
                panic!("shouldn't be possible due to above check");
            };

            // IF this is a FRAGMENT, check whether all subsequent fragments have been received
            // before merging them all together and adding to outgoing list
            todo!(); // connor

            incoming_messages.push((index, message));
            self.oldest_received_message_index = self.oldest_received_message_index.wrapping_add(1);
        }
    }
}
