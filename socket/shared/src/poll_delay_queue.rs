use std::collections::VecDeque;

/// A first-in-first-out queue of items that become available after a fixed
/// number of consumer polls, rather than after wall-clock time.
///
/// This is the deterministic counterpart to [`TimeQueue`](crate::TimeQueue):
/// the link conditioner stamps wall-clock deadlines, so one wall delay spans
/// a load-dependent tick count and two identical runs deliver on different
/// ticks. Counting consumer polls instead makes arrival-poll map to
/// delivery-poll identically on every run, which is what tick-determinism
/// gates (zero jitter, zero loss) actually require. Wall-clock conditioning
/// is unchanged and remains the default for real links.
#[derive(Clone)]
pub struct PollDelayQueue<T> {
    queue: VecDeque<(u64, T)>,
}

impl<T> PollDelayQueue<T> {
    /// Creates a new, empty `PollDelayQueue`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    /// Queues an item for delivery once the consumer's poll count reaches
    /// `due_poll`. Due polls must be non-decreasing across pushes (the link
    /// conditioner always stamps `poll + fixed_delay` with a non-decreasing
    /// poll), so the front is always the earliest due item and equal due
    /// polls pop in insertion order.
    pub fn push(&mut self, due_poll: u64, item: T) {
        debug_assert!(
            self.queue.back().is_none_or(|(due, _)| *due <= due_poll),
            "PollDelayQueue due polls must be non-decreasing",
        );
        self.queue.push_back((due_poll, item));
    }

    /// Returns whether the oldest queued item is due at `poll`.
    #[must_use]
    pub fn has_due(&self, poll: u64) -> bool {
        self.queue.front().is_some_and(|(due, _)| *due <= poll)
    }

    /// Pops the oldest item due at `poll`, or `None` when nothing is due.
    #[must_use]
    pub fn pop_due(&mut self, poll: u64) -> Option<T> {
        if self.has_due(poll) {
            if let Some((_, item)) = self.queue.pop_front() {
                return Some(item);
            }
        }
        None
    }

    /// Returns the number of queued (not yet due or not yet popped) items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns whether no items are queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl<T> Default for PollDelayQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::PollDelayQueue;

    #[test]
    fn item_is_withheld_until_its_due_poll() {
        let mut queue = PollDelayQueue::new();
        queue.push(3, "late");
        assert!(!queue.has_due(0));
        assert!(!queue.has_due(2));
        assert_eq!(queue.pop_due(2), None);
        assert!(queue.has_due(3));
        assert_eq!(queue.pop_due(3), Some("late"));
        assert!(queue.is_empty());
    }

    #[test]
    fn equal_due_polls_pop_in_insertion_order() {
        let mut queue = PollDelayQueue::new();
        queue.push(5, 1);
        queue.push(5, 2);
        queue.push(5, 3);
        assert_eq!(queue.pop_due(5), Some(1));
        assert_eq!(queue.pop_due(5), Some(2));
        assert_eq!(queue.pop_due(5), Some(3));
        assert_eq!(queue.pop_due(5), None);
    }

    #[test]
    fn staggered_dues_pop_in_due_order() {
        let mut queue = PollDelayQueue::new();
        queue.push(4, "first");
        queue.push(10, "second");
        assert_eq!(queue.pop_due(4), Some("first"));
        assert_eq!(queue.pop_due(9), None);
        assert!(queue.has_due(10));
        assert_eq!(queue.pop_due(10), Some("second"));
        assert!(queue.is_empty());
    }

    #[test]
    fn len_tracks_queued_items() {
        let mut queue: PollDelayQueue<u8> = PollDelayQueue::new();
        assert_eq!(queue.len(), 0);
        queue.push(1, 7);
        queue.push(9, 8);
        assert_eq!(queue.len(), 2);
        let _ = queue.pop_due(1);
        assert_eq!(queue.len(), 1);
    }
}
