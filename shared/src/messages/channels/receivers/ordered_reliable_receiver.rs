use std::collections::VecDeque;

use crate::{
    messages::channels::receivers::reliable_message_receiver::{
        ReceiverArranger, ReliableMessageReceiver,
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

    /// Creates a new `OrderedReliableReceiver` bounded by `max_receive_window`.
    pub fn with_window(max_receive_window: Option<u16>) -> Self {
        Self::with_arranger_and_window(
            OrderedArranger {
                messages_received: 0,
                buffer: VecDeque::new(),
            },
            max_receive_window,
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
        let start = start_message_index.wrapping_sub(self.messages_received) as usize;
        let span = end_message_index.wrapping_sub(start_message_index) as usize + 1;

        // Validate the entire range before writing any slot. Peer-controlled
        // ranges can overlap an ordinary message or a prior fragment sequence.
        if crate::sequence_less_than(start_message_index, self.messages_received)
            || start + span > u16::MAX as usize + 1
            || self
                .buffer
                .iter()
                .skip(start)
                .take(span)
                .any(|(_, slot)| !slot.is_not_received())
        {
            log::warn!("Discarding stale or overlapping ordered reliable message range.");
            return output;
        }
        while self.buffer.len() < start + span {
            let index = self
                .messages_received
                .wrapping_add(self.buffer.len() as u16);
            self.buffer.push_back((index, MessageSlot::NotReceived));
        }
        self.buffer[start].1 = MessageSlot::Received(message);
        for offset in 1..span {
            self.buffer[start + offset].1 = MessageSlot::PreviousFragment;
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

    #[test]
    fn messaging_16c_t2_overlapping_range_is_dropped_atomically() {
        let mut arranger = OrderedArranger::new();
        assert!(arranger.process(7, 7, message()).is_empty());
        assert!(arranger.process(5, 7, message()).is_empty());
        assert!(arranger.process(7, 7, message()).is_empty());
        let mut delivered = 0;
        for index in 0..7 {
            delivered += arranger.process(index, index, message()).len();
        }
        assert_eq!(delivered, 8, "rejected range must not occupy slots 5 or 6");
        assert!(arranger.buffer.is_empty());
        assert!(arranger.process(7, 7, message()).is_empty());
        assert_eq!(arranger.process(8, 8, message()).len(), 1);
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
