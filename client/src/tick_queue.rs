use std::{cmp::Ordering, collections::BinaryHeap};

use naia_shared::sequence_greater_than;

/// A queue for items marked by tick, will only ever pop items from the queue if
/// the tick has elapsed
#[derive(Debug)]
pub struct TickQueue<T: Eq + PartialEq> {
    queue: BinaryHeap<ItemContainer<T>>,
}

impl<T: Eq + PartialEq> TickQueue<T> {
    /// Create a new TimeQueue
    pub fn new() -> Self {
        TickQueue {
            queue: BinaryHeap::new(),
        }
    }

    /// Adds an item to the queue marked by tick
    pub fn add_item(&mut self, tick: u16, item: T) {
        self.queue.push(ItemContainer { tick, item });
    }

    /// Returns whether or not there is an item that is ready to be returned
    fn has_item(&self, current_tick: u16) -> bool {
        if self.queue.len() == 0 {
            return false;
        }
        if let Some(item) = self.queue.peek() {
            return current_tick == item.tick || sequence_greater_than(current_tick, item.tick);
        }
        return false;
    }

    /// Pops an item from the queue if the tick has elapsed
    pub fn pop_item(&mut self, current_tick: u16) -> Option<(u16, T)> {
        if self.has_item(current_tick) {
            if let Some(container) = self.queue.pop() {
                return Some((container.tick, container.item));
            }
        }
        return None;
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ItemContainer<T: Eq + PartialEq> {
    pub tick: u16,
    pub item: T,
}

impl<T: Eq + PartialEq> Ord for ItemContainer<T> {
    fn cmp(&self, other: &ItemContainer<T>) -> Ordering {
        if self.tick == other.tick {
            return Ordering::Equal;
        }
        return if sequence_greater_than(other.tick, self.tick) {
            Ordering::Greater
        } else {
            Ordering::Less
        };
    }
}

impl<T: Eq + PartialEq> PartialOrd for ItemContainer<T> {
    fn partial_cmp(&self, other: &ItemContainer<T>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
