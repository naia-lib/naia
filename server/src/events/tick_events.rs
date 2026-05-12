use std::vec::IntoIter;

use naia_shared::Tick;

/// Event container for server-tick events returned by [`Server::take_tick_events`](crate::Server::take_tick_events).
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

    /// Returns `true` if no tick events are pending.
    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// Drains and returns all tick events of type `V`.
    pub fn read<V: TickEventType>(&mut self) -> V::Iter {
        V::iter(self)
    }

    /// Returns `true` if at least one tick event of type `V` is pending.
    pub fn has<V: TickEventType>(&self) -> bool {
        V::has(self)
    }

    // Crate-public

    pub(crate) fn push_tick(&mut self, tick: Tick) {
        self.ticks.push(tick);
        self.empty = false;
    }
}

/// Marker trait for event types that can be read from [`TickEvents`].
pub trait TickEventType {
    /// Iterator type yielded by [`TickEvents::read`].
    type Iter;

    /// Drains all events of this type from the container.
    fn iter(events: &mut TickEvents) -> Self::Iter;

    /// Returns `true` if at least one event of this type is pending.
    fn has(events: &TickEvents) -> bool;
}

// Tick Event
/// Fires once per server-authoritative game tick; carries the current [`Tick`] value.
pub struct TickEvent;
impl TickEventType for TickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut TickEvents) -> Self::Iter {
        let list = std::mem::take(&mut events.ticks);
        IntoIterator::into_iter(list)
    }

    fn has(events: &TickEvents) -> bool {
        !events.ticks.is_empty()
    }
}
