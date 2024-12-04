use std::collections::VecDeque;

use crate::{
    messages::channels::receivers::reliable_message_receiver::{
        ReceiverArranger, ReliableMessageReceiver,
    },
    types::MessageIndex,
    MessageContainer,
};

// OrderedReliableReceiver
pub type OrderedReliableReceiver = ReliableMessageReceiver<OrderedArranger>;

impl OrderedReliableReceiver {
    pub fn new() -> Self {
        Self::with_arranger(OrderedArranger {
            messages_received: 0,
            buffer: VecDeque::new(),
        })
    }
}

enum MessageSlot {
    NotReceived,
    Received(MessageContainer),
    PreviousFragment,
}

impl MessageSlot {
    fn is_not_received(&self) -> bool {
        match self {
            MessageSlot::NotReceived => true,
            _ => false,
        }
    }
}

// OrderedArranger
pub struct OrderedArranger {
    buffer: VecDeque<(MessageIndex, MessageSlot)>,
    messages_received: MessageIndex,
}

impl ReceiverArranger for OrderedArranger {
    fn process(
        &mut self,
        start_message_index: MessageIndex,
        end_message_index: MessageIndex,
        message: MessageContainer,
    ) -> Vec<MessageContainer> {

        let mut output = Vec::new();
        let mut current_index = 0;

        // Put message where it needs to go in buffer
        loop {
            if current_index < self.buffer.len() {
                let Some((old_message_index, old_message)) = self.buffer.get_mut(current_index) else {
                    panic!("Buffer should be instantiated to slot {:?} !", start_message_index);
                };
                let old_message_index = *old_message_index;
                if old_message_index == start_message_index {
                    if old_message.is_not_received() {

                        *old_message = MessageSlot::Received(message);

                        let mut current_message_index = start_message_index;
                        while current_message_index != end_message_index {
                            current_index = current_index.wrapping_add(1);
                            let Some((old_message_index, old_message)) = self.buffer.get_mut(current_index) else {
                                panic!("Buffer should be instantiated to slot {:?} !", old_message_index);
                            };
                            let old_message_index = *old_message_index;
                            current_message_index = old_message_index;
                            if old_message.is_not_received() {
                                *old_message = MessageSlot::PreviousFragment;
                            } else {
                                panic!("Buffer should not have received message in slot {:?} !", old_message_index);
                            }
                        }

                        break;
                    } else {
                        panic!("Buffer should not have received message in slot {:?} !", old_message_index);
                    }
                }
            } else {
                let next_message_index = self
                    .messages_received
                    .wrapping_add(current_index as u16);

                if next_message_index == start_message_index {
                    self.buffer.push_back((next_message_index, MessageSlot::Received(message)));

                    let mut next_message_index = next_message_index;
                    while next_message_index != end_message_index {
                        next_message_index = next_message_index.wrapping_add(1);
                        self.buffer.push_back((next_message_index, MessageSlot::PreviousFragment));
                    }

                    break;
                } else {
                    self.buffer.push_back((next_message_index, MessageSlot::NotReceived));
                    // keep filling up buffer
                }
            }

            current_index += 1;
        }

        // Pop messages out in order
        loop {
            let Some((_, MessageSlot::Received(_))) = self.buffer.front() else {
                // no more messages, return
                return output;
            };
            let Some((_, MessageSlot::Received(message))) = self.buffer.pop_front() else {
                panic!("shouldn't be possible due to above check");
            };

            output.push(message);
            self.messages_received = self.messages_received.wrapping_add(1);

            while let Some((_, MessageSlot::PreviousFragment)) = self.buffer.front() {
                self.messages_received = self.messages_received.wrapping_add(1);
                self.buffer.pop_front();
            }
        }
    }
}
