//! # `EngineConfig` – compile‑time tuning knobs
//!
//! This tiny struct distills the **numeric parameters** that govern
//! how the sync engine manages its *sliding window* of reliable, unordered
//! `MessageIndex` values.  They are set **once at compile time** and cloned
//! into every `Engine` instance; no run‑time mutation is allowed, keeping the
//! hot path branch‑free.
//!
//! ### Why this formula?
//! Because `max_in_flight` is the maximum contiguous block that may be
//! outstanding.  By flushing when we are `max_in_flight` counts away from the
//! wrap, we ensure the next batch of IDs will never collide with IDs still
//! referenced by the receiver, even under worst‑case re‑ordering.

pub struct EngineConfig {
    /// *Upper bound on the count of un‑ACKed packets per peer.*
    /// - **Constraint**: `max_in_flight < 32 768` (½ of the `u16` range) so that
    ///   simple *“less‑than or equal with wrap‑around”* comparisons remain
    ///   unambiguous.
    pub max_in_flight: u16,
    /// *Guard‑band distance from the sequence‑number wrap point (65 536).*
    /// When the **oldest living packet ID ≥ flush_threshold**, the sender forces a
    /// flush of pending data **on the sender** before it reuses IDs that might
    /// still be referenced by the receiver, guaranteeing the *“unique ID across
    /// the sliding window”* invariant.  The receiver treats wrap‑around as an
    /// ordinary comparison; no state reset occurs.
    pub flush_threshold: u16,
}

impl Default for EngineConfig {
    fn default() -> Self {
        // Upper bound on un-ACKed packets (< 32_768).
        let max_in_flight: u16 = 32_767;

        // Guard-band threshold where we flush backlog near wrap-around.
        let flush_threshold: u16 = (65_536u32 - max_in_flight as u32) as u16;

        Self {
            max_in_flight,
            flush_threshold,
        }
    }
}
