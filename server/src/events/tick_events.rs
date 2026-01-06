use std::vec::IntoIter;

use naia_shared::Tick;

pub struct TickEvents {
    ticks: Vec<Tick>,
    empty: bool,
}

impl TickEvents {
    pub(crate) fn new() -> Self {
        Self {
            ticks: Vec::new(),
            empty: true,
        }
    }

    // Public

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn read<V: TickEventType>(&mut self) -> V::Iter {
        return V::iter(self);
    }

    pub fn has<V: TickEventType>(&self) -> bool {
        return V::has(self);
    }

    // Crate-public

    pub(crate) fn push_tick(&mut self, tick: Tick) {
        self.ticks.push(tick);
        self.empty = false;
    }
}

// Event Trait
pub trait TickEventType {
    type Iter;

    fn iter(events: &mut TickEvents) -> Self::Iter;

    fn has(events: &TickEvents) -> bool;
}

// Tick Event
pub struct TickEvent;
impl TickEventType for TickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut TickEvents) -> Self::Iter {
        let list = std::mem::take(&mut events.ticks);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &TickEvents) -> bool {
        !events.ticks.is_empty()
    }
}
