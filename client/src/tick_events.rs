use std::vec::IntoIter;

use naia_shared::Tick;

/// Collects client and server tick events emitted during a single frame for typed iteration.
pub struct TickEvents {
    client_ticks: Vec<Tick>,
    server_ticks: Vec<Tick>,
    empty: bool,
}

impl Default for TickEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl TickEvents {
    pub(crate) fn new() -> Self {
        Self {
            client_ticks: Vec::new(),
            server_ticks: Vec::new(),
            empty: true,
        }
    }

    /// Returns `true` if no tick events have been queued this frame.
    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// Drains and returns an iterator over tick events of type `V`.
    pub fn read<V: TickEvent>(&mut self) -> V::Iter {
        V::iter(self)
    }

    /// Returns `true` if at least one tick event of type `V` is queued.
    pub fn has<V: TickEvent>(&self) -> bool {
        V::has(self)
    }

    pub(crate) fn push_client_tick(&mut self, tick: Tick) {
        self.client_ticks.push(tick);
        self.empty = false;
    }

    pub(crate) fn push_server_tick(&mut self, tick: Tick) {
        self.server_ticks.push(tick);
        self.empty = false;
    }

    pub(crate) fn clear(&mut self) {
        self.client_ticks.clear();
        self.server_ticks.clear();
        self.empty = true;
    }
}

/// Type-indexed tick event; implemented by [`ClientTickEvent`] and [`ServerTickEvent`].
pub trait TickEvent {
    /// Iterator type returned from [`TickEvents::read`].
    type Iter;

    /// Drains tick events of this variant out of `events` and returns an iterator over them.
    fn iter(events: &mut TickEvents) -> Self::Iter;

    /// Returns `true` if `events` contains at least one event of this variant.
    fn has(events: &TickEvents) -> bool;
}

/// Fired each client-side simulation tick; iterate via `tick_events.read::<ClientTickEvent>()`.
pub struct ClientTickEvent;
impl TickEvent for ClientTickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut TickEvents) -> Self::Iter {
        let list = std::mem::take(&mut events.client_ticks);
        IntoIterator::into_iter(list)
    }

    fn has(events: &TickEvents) -> bool {
        !events.client_ticks.is_empty()
    }
}

/// Fired each time the server's authoritative tick advances; iterate via `tick_events.read::<ServerTickEvent>()`.
pub struct ServerTickEvent;
impl TickEvent for ServerTickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut TickEvents) -> Self::Iter {
        let list = std::mem::take(&mut events.server_ticks);
        IntoIterator::into_iter(list)
    }

    fn has(events: &TickEvents) -> bool {
        !events.server_ticks.is_empty()
    }
}
