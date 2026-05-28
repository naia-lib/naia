use std::collections::VecDeque;

use naia_shared::{OwnedBitReader, Tick};

use crate::connection::tick_queue::TickQueue;

/// Configuration type for jitter buffer behavior
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JitterBufferType {
    /// Use the real jitter buffer with tick-based ordering
    Real,
    /// Bypass the jitter buffer and process packets immediately
    Bypass,
}

/// Runtime jitter buffer implementation
/// Can be either a real tick-ordered queue or a simple bypass queue
pub enum JitterBuffer {
    /// Real jitter buffer that orders packets by server tick
    Real(TickQueue<OwnedBitReader>),
    /// Bypass buffer that processes packets immediately in FIFO order
    Bypass(VecDeque<(Tick, OwnedBitReader)>),
}

impl JitterBuffer {
    /// Create a new jitter buffer based on the configuration type
    pub fn new(jitter_buffer_type: JitterBufferType) -> Self {
        match jitter_buffer_type {
            JitterBufferType::Real => JitterBuffer::Real(TickQueue::new()),
            JitterBufferType::Bypass => JitterBuffer::Bypass(VecDeque::new()),
        }
    }

    /// Add an item to the buffer
    pub fn add_item(&mut self, tick: Tick, item: OwnedBitReader) {
        match self {
            JitterBuffer::Real(queue) => {
                queue.add_item(tick, item);
            }
            JitterBuffer::Bypass(queue) => {
                queue.push_back((tick, item));
            }
        }
    }

    /// Pop an item from the buffer if its server tick has elapsed (`tick <=
    /// current_tick`). Both modes honor this bound — under `Bypass` the storage
    /// is a FIFO `VecDeque`, but the bound is still required for correctness:
    /// applying a snapshot for tick T+1 before the confirmed timeline has
    /// re-simulated tick T pollutes the world with future-tick state and breaks
    /// the deterministic re-sim. The receiving-tick cap in
    /// `time_manager::collect_ticks` keeps `current_tick <= last_delivered`, so
    /// this peek-then-pop is sufficient (packets arrive in server-tick order
    /// over a reliable in-order channel, so the front of the FIFO is the oldest
    /// undelivered tick).
    pub fn pop_item(&mut self, current_tick: Tick) -> Option<(Tick, OwnedBitReader)> {
        match self {
            JitterBuffer::Real(queue) => queue.pop_item(current_tick),
            JitterBuffer::Bypass(queue) => {
                let front_tick = queue.front().map(|(t, _)| *t)?;
                if naia_shared::sequence_greater_than(front_tick, current_tick) {
                    return None;
                }
                queue.pop_front()
            }
        }
    }
}
