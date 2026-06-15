use std::{collections::VecDeque, mem, time::Duration};

use naia_socket_shared::Instant;

use crate::{messages::channels::senders::channel_sender::ChannelSender, types::MessageIndex};

/// Retransmit-on-timeout sender that tracks unacknowledged messages and re-queues them after an RTT-based interval.
///
/// `sending_messages` is the single ordered source of truth (insertion order ==
/// logical sequence order, including across the u16 wrap boundary). Every tick,
/// [`collect_messages`](ReliableSender::collect_messages) rebuilds
/// `outgoing_messages` as a fresh, derived *view* of the entries due for
/// (re)transmission — never mutating it incrementally — so the view is always in
/// sequence order by construction, with no sort and no wrap-around hazard. A
/// message's `last_sent` timestamp is stamped only when it is *actually written*
/// into a packet (see [`mark_written`](ReliableSender::mark_written)), not when
/// it is staged — so a message that doesn't fit the packet (bandwidth/size
/// truncation) stays "due" and is re-collected next tick instead of having its
/// resend timer silently reset.
pub struct ReliableSender<P: Send + Sync> {
    rtt_resend_factor: f32,
    sending_messages: VecDeque<Option<(MessageIndex, Option<Instant>, P)>>,
    next_send_message_index: MessageIndex,
    pub(crate) outgoing_messages: VecDeque<(MessageIndex, P)>,
    // Timestamp of the collect_messages pass that built the current
    // `outgoing_messages` view, applied to each message's `last_sent` when it is
    // written (collect and the multi-packet write loop share one tick, so this
    // is the correct send time for everything staged this tick).
    collected_at: Option<Instant>,
    // Earliest `last_sent` across all currently-pending entries, recomputed on
    // each collect_messages pass. Combined with `has_unsent`, lets
    // collect_messages short-circuit when nothing is due for resend — avoiding
    // an O(N) scan of `sending_messages` every tick even when the channel is
    // idle (e.g. 10k entity-spawn commands waiting on acks).
    min_last_sent: Option<Instant>,
    has_unsent: bool,
    max_queue_depth: Option<usize>,
}

impl<P: Send + Sync> ReliableSender<P> {
    /// Creates a `ReliableSender` with the given RTT resend factor and optional queue-depth cap.
    pub fn new(rtt_resend_factor: f32, max_queue_depth: Option<usize>) -> Self {
        Self {
            rtt_resend_factor,
            next_send_message_index: 0,
            sending_messages: VecDeque::new(),
            outgoing_messages: VecDeque::new(),
            collected_at: None,
            min_last_sent: None,
            has_unsent: false,
            max_queue_depth,
        }
    }

    /// Stamp `last_sent` on the entries that were actually written into a packet.
    ///
    /// Called by the message sender after `IndexedMessageWriter` reports which
    /// staged indices it serialized. `written` is in ascending sequence order (a
    /// prefix of the ordered `outgoing_messages` view), and `sending_messages` is
    /// in that same order, so a single forward merge-walk stamps them — matching
    /// purely by index equality, which is wrap-safe (no `<` comparison of raw
    /// u16s). Entries left unwritten keep their prior `last_sent` and remain due.
    pub(crate) fn mark_written(&mut self, written: &[MessageIndex]) {
        let Some(now) = self.collected_at.clone() else {
            return;
        };
        let mut next = written.iter();
        let mut target = next.next();
        for slot in self.sending_messages.iter_mut().flatten() {
            let Some(&want) = target else {
                break;
            };
            if slot.0 == want {
                slot.1 = Some(now.clone());
                target = next.next();
            }
        }
    }

    fn cleanup_sent_messages(&mut self) {
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
    }

    /// Drains and returns all messages currently staged for transmission this tick.
    pub fn take_next_messages(&mut self) -> VecDeque<(MessageIndex, P)> {
        mem::take(&mut self.outgoing_messages)
    }

    /// Acknowledges delivery of `message_index`, removing it from the retransmit buffer and returning the message.
    // Called when a message has been delivered
    // If this message has never been delivered before, will clear from the outgoing
    // buffer and return the message previously there
    pub fn deliver_message(&mut self, message_index: &MessageIndex) -> Option<P> {
        let mut index = 0;
        let mut found = false;

        loop {
            if index == self.sending_messages.len() {
                return None;
            }

            if let Some(Some((old_message_index, _, _))) = self.sending_messages.get(index) {
                if *message_index == *old_message_index {
                    found = true;
                }
            }

            if found {
                // replace found message with nothing
                let container = self.sending_messages.get_mut(index).unwrap();
                let output = container.take();

                self.cleanup_sent_messages();

                // stop loop
                return output.map(|(_, _, message)| message);
            }

            index += 1;
        }
    }
}

impl<P: Send + Sync + Clone> ChannelSender<P> for ReliableSender<P> {
    fn send_message(&mut self, message: P) -> bool {
        if let Some(max) = self.max_queue_depth {
            if self.sending_messages.len() >= max {
                return false;
            }
        }
        self.sending_messages
            .push_back(Some((self.next_send_message_index, None, message)));
        self.next_send_message_index = self.next_send_message_index.wrapping_add(1);
        self.has_unsent = true;
        true
    }

    fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        let resend_duration = Duration::from_millis((self.rtt_resend_factor * rtt_millis) as u64);

        // Fast path: no newly-queued messages and min(last_sent) + resend_duration > now
        // means nothing is due for resend. Skip the O(N) scan of sending_messages.
        if !self.has_unsent {
            if self.sending_messages.is_empty() {
                return;
            }
            if let Some(min) = self.min_last_sent.as_ref() {
                if min.elapsed(now) < resend_duration {
                    return;
                }
            }
        }

        // Rebuild the outgoing view from scratch. Any entries left over from a
        // bandwidth/size-truncated packet last tick are dropped here and
        // re-derived below from `sending_messages` — the ordered source of truth
        // — so the view is always in sequence order with no incremental append
        // (and thus no possibility of a lower index landing behind a higher one).
        self.outgoing_messages.clear();
        let mut new_min: Option<Instant> = None;
        let mut any_unsent = false;
        for (message_index, last_sent_opt, message) in self.sending_messages.iter().flatten() {
            let due = match last_sent_opt {
                Some(last_sent) => last_sent.elapsed(now) >= resend_duration,
                None => true,
            };
            if due {
                self.outgoing_messages
                    .push_back((*message_index, message.clone()));
            }
            match last_sent_opt {
                None => any_unsent = true,
                Some(t) => {
                    new_min = Some(match new_min {
                        None => t.clone(),
                        Some(cur) => {
                            if t < &cur {
                                t.clone()
                            } else {
                                cur
                            }
                        }
                    });
                }
            }
        }
        self.collected_at = Some(now.clone());
        self.min_last_sent = new_min;
        // Stays true while any message has never been written; self-corrects to
        // false on the first collect after mark_written stamps them all.
        self.has_unsent = any_unsent;
    }

    fn has_messages(&self) -> bool {
        !self.outgoing_messages.is_empty()
    }

    fn notify_message_delivered(&mut self, message_index: &MessageIndex) {
        self.deliver_message(message_index);
    }
}

#[cfg(test)]
mod tests {
    //! Retransmit-ordering + stamp-on-write contract for `ReliableSender`.
    //!
    //! These drive the REAL `collect_messages` / `mark_written` /
    //! `deliver_message` / `take_next_messages` methods — no reimplementation —
    //! and pin the sender's responsibility to feed `IndexedMessageWriter` a
    //! wrap-safe, sequence-ordered stream. `indexed_message_writer.rs` already
    //! pins the dual invariant (it panics on out-of-order input); the gap these
    //! close is that nothing asserted the *sender* never produces such input.

    use super::ReliableSender;
    use crate::messages::channels::senders::channel_sender::ChannelSender;
    use naia_socket_shared::Instant;

    // rtt_resend_factor 1.0 * rtt 100ms => resend_duration = 100ms.
    const RTT_MILLIS: f32 = 100.0;

    fn at(base: &Instant, plus_millis: u32) -> Instant {
        let mut t = base.clone();
        t.add_millis(plus_millis);
        t
    }

    fn indices(out: &std::collections::VecDeque<(u16, u16)>) -> Vec<u16> {
        out.iter().map(|(i, _)| *i).collect()
    }

    #[test]
    fn collect_feeds_messages_in_sequence_order_across_the_u16_wrap() {
        // The original bug: a raw-u16 sort put [65534, 65535, 0, 1] as
        // [0, 1, 65534, 65535], making the downstream wrapping_diff(1, 65534)
        // negative => panic. The view must preserve insertion (sequence) order,
        // which straddles the wrap correctly.
        let mut sender: ReliableSender<u16> = ReliableSender::new(1.0, None);
        sender.next_send_message_index = 65534;
        for payload in [0xAA, 0xBB, 0xCC, 0xDD] {
            sender.send_message(payload); // indices 65534, 65535, 0, 1
        }

        let now = Instant::now();
        sender.collect_messages(&now, &RTT_MILLIS);
        let out = sender.take_next_messages();

        assert_eq!(
            indices(&out),
            vec![65534, 65535, 0, 1],
            "outgoing view must stay in sequence order across the wrap boundary"
        );
    }

    #[test]
    fn only_written_messages_have_their_resend_timer_reset() {
        // Stamp-on-write: a message staged but NOT written (packet truncated by
        // bandwidth/size) must stay due, not have its resend timer silently
        // reset. The old code stamped at collect time, delaying truncated
        // leftovers by a full RTT.
        let mut sender: ReliableSender<u16> = ReliableSender::new(1.0, None);
        for _ in 0..5 {
            sender.send_message(0); // indices 0..=4
        }

        let t0 = Instant::now();
        sender.collect_messages(&t0, &RTT_MILLIS);
        let first = sender.take_next_messages();
        assert_eq!(indices(&first), vec![0, 1, 2, 3, 4]);

        // Simulate a packet that only had room for [0, 1, 2].
        sender.mark_written(&[0, 1, 2]);

        // 50ms later — less than the 100ms resend window.
        let t1 = at(&t0, 50);
        sender.collect_messages(&t1, &RTT_MILLIS);
        let second = sender.take_next_messages();
        assert_eq!(
            indices(&second),
            vec![3, 4],
            "written messages must wait out their resend window; unwritten leftovers stay due"
        );
    }

    #[test]
    fn written_messages_retransmit_once_the_resend_window_elapses() {
        let mut sender: ReliableSender<u16> = ReliableSender::new(1.0, None);
        for _ in 0..3 {
            sender.send_message(0); // indices 0..=2
        }

        let t0 = Instant::now();
        sender.collect_messages(&t0, &RTT_MILLIS);
        let _ = sender.take_next_messages();
        sender.mark_written(&[0, 1, 2]);

        // Past the resend window — all unacked messages must reappear, in order.
        let t1 = at(&t0, 200);
        sender.collect_messages(&t1, &RTT_MILLIS);
        let retransmit = sender.take_next_messages();
        assert_eq!(indices(&retransmit), vec![0, 1, 2]);
    }

    #[test]
    fn acknowledged_messages_are_never_retransmitted() {
        let mut sender: ReliableSender<u16> = ReliableSender::new(1.0, None);
        for _ in 0..4 {
            sender.send_message(0); // indices 0..=3
        }

        let t0 = Instant::now();
        sender.collect_messages(&t0, &RTT_MILLIS);
        let _ = sender.take_next_messages();
        sender.mark_written(&[0, 1, 2, 3]);

        // Ack index 1; it must drop out of the retransmit set.
        sender.deliver_message(&1);

        let t1 = at(&t0, 200);
        sender.collect_messages(&t1, &RTT_MILLIS);
        let retransmit = sender.take_next_messages();
        assert_eq!(
            indices(&retransmit),
            vec![0, 2, 3],
            "acked index 1 must not retransmit; the rest stay in order"
        );
    }
}
