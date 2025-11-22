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
            JitterBuffer::Real(queue) => queue.add_item(tick, item),
            JitterBuffer::Bypass(queue) => queue.push_back((tick, item)),
        }
    }

    /// Pop an item from the buffer if the tick has elapsed (Real mode)
    /// or immediately (Bypass mode)
    pub fn pop_item(&mut self, current_tick: Tick) -> Option<(Tick, OwnedBitReader)> {
        match self {
            JitterBuffer::Real(queue) => queue.pop_item(current_tick),
            JitterBuffer::Bypass(queue) => queue.pop_front(),
        }
    }
}

