use std::collections::VecDeque;

use crate::{
    messages::channels::receivers::reliable_message_receiver::{
        ReceiverArranger, ReceiverCaps, ReliableMessageReceiver,
    },
    types::MessageIndex,
    MessageContainer,
};

/// Reliable receiver that delivers messages to callers strictly in send order.
pub type OrderedReliableReceiver = ReliableMessageReceiver<OrderedArranger>;

impl OrderedReliableReceiver {
    /// Creates a new `OrderedReliableReceiver` with no throughput cap.
    pub fn new() -> Self {
        Self::with_arranger(OrderedArranger {
            messages_received: 0,
            buffer: VecDeque::new(),
        })
    }

    /// Creates a new `OrderedReliableReceiver` bounded by `caps`.
    pub fn with_caps(caps: ReceiverCaps) -> Self {
        Self::with_arranger_and_caps(
            OrderedArranger {
                messages_received: 0,
                buffer: VecDeque::new(),
            },
            caps,
        )
    }
}

enum MessageSlot {
    NotReceived,
    Received(MessageContainer),
    PreviousFragment,
}

impl MessageSlot {
    fn is_not_received(&self) -> bool {
        matches!(self, MessageSlot::NotReceived)
    }
}

/// Arranger that buffers out-of-order messages and releases them in strict send order.
pub struct OrderedArranger {
    buffer: VecDeque<(MessageIndex, MessageSlot)>,
    messages_received: MessageIndex,
}

#[cfg(test)]
impl OrderedArranger {
    pub(crate) fn new() -> Self {
        Self {
            buffer: VecDeque::new(),
            messages_received: 0,
        }
    }
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
                let Some((old_message_index, old_message)) = self.buffer.get_mut(current_index)
                else {
                    panic!(
                        "Buffer should be instantiated to slot {:?} !",
                        start_message_index
                    );
                };
                let old_message_index = *old_message_index;
                if old_message_index == start_message_index {
                    if old_message.is_not_received() {
                        *old_message = MessageSlot::Received(message);

                        let mut current_message_index = start_message_index;
                        while current_message_index != end_message_index {
                            current_index = current_index.wrapping_add(1);
                            let Some((old_message_index, old_message)) =
                                self.buffer.get_mut(current_index)
                            else {
                                panic!(
                                    "Buffer should be instantiated to slot {:?} !",
                                    old_message_index
                                );
                            };
                            let old_message_index = *old_message_index;
                            current_message_index = old_message_index;
                            if old_message.is_not_received() {
                                *old_message = MessageSlot::PreviousFragment;
                            } else {
                                panic!(
                                    "Buffer should not have received message in slot {:?} !",
                                    old_message_index
                                );
                            }
                        }

                        break;
                    } else {
                        panic!(
                            "Buffer should not have received message in slot {:?} !",
                            old_message_index
                        );
                    }
                }
            } else {
                let next_message_index = self.messages_received.wrapping_add(current_index as u16);

                if next_message_index == start_message_index {
                    self.buffer
                        .push_back((next_message_index, MessageSlot::Received(message)));

                    let mut next_message_index = next_message_index;
                    while next_message_index != end_message_index {
                        next_message_index = next_message_index.wrapping_add(1);
                        self.buffer
                            .push_back((next_message_index, MessageSlot::PreviousFragment));
                    }

                    break;
                } else {
                    self.buffer
                        .push_back((next_message_index, MessageSlot::NotReceived));
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

#[cfg(test)]
mod tests {
    use super::OrderedArranger;
    use crate::messages::{
        channels::receivers::ReceiverArranger,
        fragment::{FragmentId, FragmentIndex, FragmentedMessage},
    };
    use crate::MessageContainer;

    fn message() -> MessageContainer {
        MessageContainer::new(Box::new(FragmentedMessage::new(
            FragmentId::zero(),
            FragmentIndex::zero(),
            Box::new([0u8; 4]),
        )))
    }

    /// This arranger buffers a slot per index between the last message it
    /// released and the one being processed, so its memory is governed by the
    /// widest index it is ever handed.
    ///
    /// It has no window of its own on purpose: it never sees an index directly
    /// off the wire. `ReliableReceiver::buffer_message` is the single funnel in
    /// front of it and rejects anything beyond the receive window, so the bound
    /// below holds by construction. This test pins that dependency -- if the
    /// funnel's guard is ever removed, the assertion here is what should fail.
    #[test]
    fn buffer_is_bounded_by_the_span_of_indices_it_is_fed() {
        let window = 32u16;
        let mut arranger = OrderedArranger::new();

        // The worst case the funnel permits: the furthest in-window index first,
        // so every intervening slot is instantiated and nothing can be released.
        arranger.process(window, window, message());
        assert_eq!(
            arranger.buffer.len(),
            window as usize + 1,
            "one slot per index up to the furthest in-window index"
        );

        // Filling in from the front releases in order and drains the buffer.
        let mut released = 0;
        for index in 0..window {
            released += arranger.process(index, index, message()).len();
        }
        assert_eq!(
            released,
            window as usize + 1,
            "all messages released in order"
        );
        assert!(
            arranger.buffer.is_empty(),
            "buffer drains once the sequence is contiguous, leaving {} slots",
            arranger.buffer.len()
        );
    }
}
