use std::vec::IntoIter;

use naia_shared::Tick;

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

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn read<V: TickEvent>(&mut self) -> V::Iter {
        return V::iter(self);
    }

    pub fn has<V: TickEvent>(&self) -> bool {
        return V::has(self);
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

// Event Trait
pub trait TickEvent {
    type Iter;

    fn iter(events: &mut TickEvents) -> Self::Iter;

    fn has(events: &TickEvents) -> bool;
}

// Client Tick Event
pub struct ClientTickEvent;
impl TickEvent for ClientTickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut TickEvents) -> Self::Iter {
        let list = std::mem::take(&mut events.client_ticks);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &TickEvents) -> bool {
        !events.client_ticks.is_empty()
    }
}

// Server Tick Event
pub struct ServerTickEvent;
impl TickEvent for ServerTickEvent {
    type Iter = IntoIter<Tick>;

    fn iter(events: &mut TickEvents) -> Self::Iter {
        let list = std::mem::take(&mut events.server_ticks);
        return IntoIterator::into_iter(list);
    }

    fn has(events: &TickEvents) -> bool {
        !events.server_ticks.is_empty()
    }
}
