use std::{cmp::Ordering, collections::BinaryHeap};

use super::Instant;

/// A queue for items marked by time, will only ever pop items from the queue if
/// the time passes
#[derive(Clone)]
pub struct TimeQueue<T: Eq + PartialEq> {
    queue: BinaryHeap<ItemContainer<T>>,
    next_sequence: u64,
}

#[allow(clippy::new_without_default)]
impl<T: Eq + PartialEq> TimeQueue<T> {
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::default(),
            next_sequence: 0,
        }
    }
}

impl<T: Eq + PartialEq> TimeQueue<T> {
    /// Adds an item to the queue marked by time
    pub fn add_item(&mut self, instant: Instant, item: T) {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("TimeQueue insertion sequence exhausted");
        self.queue.push(ItemContainer {
            instant,
            sequence,
            item,
        });
    }

    /// Returns whether or not there is an item whose time has elapsed on the queue
    pub fn has_item(&self, now: &Instant) -> bool {
        if self.queue.is_empty() {
            return false;
        }
        if let Some(item) = self.queue.peek() {
            // item's instant has passed, so it's ready to be returned

            let will_pop = now.is_after(&item.instant);

            return will_pop;
        }
        false
    }

    /// Pops an item from the queue if it's time has elapsed
    pub fn pop_item(&mut self, now: &Instant) -> Option<T> {
        if self.has_item(now) {
            if let Some(container) = self.queue.pop() {
                return Some(container.item);
            }
        }
        None
    }

    /// Peeks at the top level item container on the queue
    pub fn peek_entry(&self) -> Option<&ItemContainer<T>> {
        self.queue.peek()
    }

    /// Returns the length of the underlying queue
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Checks if the underlying queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[derive(Clone)]
pub struct ItemContainer<T: Eq + PartialEq> {
    pub instant: Instant,
    sequence: u64,
    pub item: T,
}

impl<T: Eq + PartialEq> PartialEq for ItemContainer<T> {
    fn eq(&self, other: &Self) -> bool {
        self.instant == other.instant && self.sequence == other.sequence
    }
}

impl<T: Eq + PartialEq> Eq for ItemContainer<T> {}

impl<T: Eq + PartialEq> Ord for ItemContainer<T> {
    fn cmp(&self, other: &ItemContainer<T>) -> Ordering {
        other
            .instant
            .cmp(&self.instant)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

impl<T: Eq + PartialEq> PartialOrd for ItemContainer<T> {
    fn partial_cmp(&self, other: &ItemContainer<T>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(all(test, feature = "test_time"))]
mod tests {
    use super::{Instant, TimeQueue};
    use crate::TestClock;

    #[derive(Clone, Eq, PartialEq)]
    struct NonOrdPacket(u8);

    #[test]
    fn equal_instant_packets_pop_in_insertion_order() {
        TestClock::init(0);
        let instant = Instant::now();
        let mut queue = TimeQueue::new();
        for packet in 1..=5 {
            queue.add_item(instant.clone(), packet);
        }

        TestClock::advance(1);
        let now = Instant::now();
        for packet in 1..=5 {
            assert_eq!(queue.pop_item(&now), Some(packet));
        }
        assert_eq!(queue.pop_item(&now), None);
    }

    #[test]
    fn payloads_only_need_equality() {
        TestClock::init(0);
        let instant = Instant::now();
        let mut queue = TimeQueue::new();
        queue.add_item(instant, NonOrdPacket(1));

        TestClock::advance(1);
        let now = Instant::now();
        assert_eq!(queue.pop_item(&now).map(|packet| packet.0), Some(1));
    }
}
