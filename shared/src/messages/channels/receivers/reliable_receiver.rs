use std::collections::VecDeque;

use log::warn;

use crate::{sequence_less_than, MessageIndex};

/// Widest receive window we will ever honour, in message indices.
///
/// `MessageIndex` is a `u16` interpreted as a wrapping sequence, so "ahead of"
/// only has meaning for half the space. A window at the half-way point is
/// therefore the widest one that is still well defined, and it is what an
/// unconfigured channel falls back to.
const MAX_RECEIVE_WINDOW: u16 = u16::MAX / 2;

pub struct ReliableReceiver<M> {
    oldest_received_message_index: MessageIndex,
    record: VecDeque<(MessageIndex, bool)>,
    incoming_messages: Vec<(MessageIndex, M)>,
    max_receive_window: Option<u16>,
}

impl<M> ReliableReceiver<M> {
    /// Creates a receiver with no receive window -- it will buffer up to half the
    /// index space on demand. Prefer [`with_window`](Self::with_window).
    pub fn new() -> Self {
        Self::with_window(None)
    }

    /// Creates a receiver that refuses message indices more than `max_receive_window`
    /// ahead of the oldest index it is still waiting for.
    ///
    /// See [`buffer_message`](Self::buffer_message) for why the window is safe to
    /// enforce by dropping, and where the value comes from.
    pub fn with_window(max_receive_window: Option<u16>) -> Self {
        Self {
            oldest_received_message_index: 0,
            record: VecDeque::default(),
            incoming_messages: Vec::default(),
            max_receive_window: max_receive_window.map(|w| w.min(MAX_RECEIVE_WINDOW)),
        }
    }

    pub(crate) fn buffer_message(&mut self, message_index: MessageIndex, message: M) {
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

        // Enforce the receive window before the slot-filling loop below, because
        // that loop instantiates one record entry per index between the oldest
        // index we are still waiting for and this one -- and `message_index` is
        // read off the wire. Unbounded, a peer's *first* packet can claim index
        // 32767 and make us materialise 32768 slots that then stay resident,
        // per channel, per connection.
        //
        // Dropping here does not weaken reliability, because an honest peer
        // cannot reach this branch. Its sender holds at most `max_queue_depth`
        // messages in flight (`ReliableSender::send_message` refuses beyond
        // that), it assigns indices consecutively, and it cannot retire an index
        // until we acknowledge it -- so the span between our oldest outstanding
        // index and anything it may legitimately send is bounded by exactly that
        // depth, which is where the window comes from. An index past the window
        // is therefore a protocol violation, and the only data lost is data a
        // conforming peer would never have sent.
        //
        // Note this is a drop rather than a connection error on purpose: the ack
        // for the containing packet is recorded before the payload is parsed, so
        // there is no path here that can make the peer retransmit.
        if let Some(window) = self.max_receive_window {
            let indices_ahead = message_index.wrapping_sub(self.oldest_received_message_index);
            if indices_ahead > window {
                warn!(
                    "reliable channel: message index {} is {} ahead of the oldest outstanding index {} (window {}); dropping. An honest peer cannot exceed the window -- this indicates a misbehaving or hostile peer.",
                    message_index, indices_ahead, self.oldest_received_message_index, window
                );
                return;
            }
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

    pub(crate) fn receive_messages(&mut self) -> Vec<(MessageIndex, M)> {
        std::mem::take(&mut self.incoming_messages)
    }
}

#[cfg(test)]
mod tests {
    use super::{ReliableReceiver, MAX_RECEIVE_WINDOW};

    /// The record grows one slot per index between the oldest outstanding index
    /// and the incoming one, and `message_index` is read off the wire. Without a
    /// window a peer's very first packet materialises half the index space.
    #[test]
    fn forged_index_cannot_inflate_the_record() {
        let mut receiver = ReliableReceiver::<u32>::with_window(Some(1024));
        receiver.buffer_message(32767, 1);

        assert!(
            receiver.record.len() <= 1025,
            "record grew to {} slots from a single out-of-window packet",
            receiver.record.len()
        );
        assert!(
            receiver.receive_messages().is_empty(),
            "an out-of-window message must not be delivered"
        );
    }

    /// Rejecting out-of-window indices must not cost the peer anything it could
    /// legitimately have sent: everything inside the window still arrives, in or
    /// out of order.
    #[test]
    fn every_index_within_the_window_is_accepted() {
        let window = 16u16;
        let mut receiver = ReliableReceiver::<u16>::with_window(Some(window));

        // Arrive back-to-front, entirely within the window, leaving index 0 last
        // so nothing is retired until the very end.
        for index in (0..=window).rev() {
            receiver.buffer_message(index, index);
        }

        let mut delivered: Vec<u16> = receiver
            .receive_messages()
            .into_iter()
            .map(|(index, _)| index)
            .collect();
        delivered.sort_unstable();
        let expected: Vec<u16> = (0..=window).collect();
        assert_eq!(delivered, expected);
    }

    /// The boundary itself: exactly `window` ahead is the furthest an honest
    /// sender bounded by `max_queue_depth` can reach, so it must be accepted,
    /// and one past it must not be.
    #[test]
    fn the_window_boundary_is_inclusive() {
        let window = 8u16;

        let mut accepts = ReliableReceiver::<u8>::with_window(Some(window));
        accepts.buffer_message(window, 1);
        assert_eq!(accepts.receive_messages().len(), 1, "index == window");

        let mut rejects = ReliableReceiver::<u8>::with_window(Some(window));
        rejects.buffer_message(window + 1, 1);
        assert!(rejects.receive_messages().is_empty(), "index == window + 1");
    }

    /// The window slides: once early indices are retired, indices that were out
    /// of window before come into range.
    #[test]
    fn the_window_advances_with_the_oldest_index() {
        let window = 4u16;
        let mut receiver = ReliableReceiver::<u16>::with_window(Some(window));

        receiver.buffer_message(5, 5);
        assert!(receiver.receive_messages().is_empty(), "5 is past window 4");

        // Retire 0..=1, moving the oldest outstanding index to 2.
        for index in 0..=1u16 {
            receiver.buffer_message(index, index);
        }
        assert_eq!(receiver.receive_messages().len(), 2);
        assert_eq!(receiver.oldest_received_message_index, 2);

        // 5 is now only 3 ahead, so it is inside the window.
        receiver.buffer_message(5, 5);
        assert_eq!(receiver.receive_messages().len(), 1, "5 is now in window");
    }

    /// `None` preserves the old unbounded behaviour, but still cannot exceed the
    /// half of the index space where "ahead" is meaningful.
    #[test]
    fn an_unset_window_falls_back_to_half_the_index_space() {
        let receiver = ReliableReceiver::<u8>::new();
        assert!(receiver.max_receive_window.is_none());

        let clamped = ReliableReceiver::<u8>::with_window(Some(u16::MAX));
        assert_eq!(clamped.max_receive_window, Some(MAX_RECEIVE_WINDOW));
    }

    /// Duplicate and already-retired indices keep their existing handling --
    /// the window guard must not disturb the sliding record.
    #[test]
    fn duplicates_and_stale_indices_are_still_ignored() {
        let mut receiver = ReliableReceiver::<u8>::with_window(Some(64));

        receiver.buffer_message(0, 1);
        receiver.buffer_message(0, 2);
        assert_eq!(receiver.receive_messages().len(), 1, "duplicate ignored");

        assert_eq!(receiver.oldest_received_message_index, 1);
        receiver.buffer_message(0, 3);
        assert!(
            receiver.receive_messages().is_empty(),
            "stale index ignored"
        );
    }
}
