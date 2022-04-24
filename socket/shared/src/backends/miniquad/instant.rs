extern "C" {
    pub fn naia_now() -> f64;
}

use std::{cmp::Ordering, time::Duration};

/// Represents a specific moment in time
#[derive(Clone, PartialEq, PartialOrd)]
pub struct Instant {
    inner: f64,
}

impl Instant {
    /// Creates an Instant from the moment the method is called
    pub fn now() -> Self {
        unsafe { Instant { inner: naia_now() } }
    }

    /// Returns time elapsed since the Instant
    pub fn elapsed(&self) -> Duration {
        unsafe {
            let inner_duration = naia_now() - self.inner;
            let seconds: u64 = (inner_duration as u64) / 1000;
            let nanos: u32 = ((inner_duration as u32) % 1000) * 1000000;
            Duration::new(seconds, nanos)
        }
    }

    /// Returns time until the Instant occurs
    pub fn until(&self) -> Duration {
        unsafe {
            let inner_duration = self.inner - naia_now();
            let seconds: u64 = (inner_duration as u64) / 1000;
            let nanos: u32 = ((inner_duration as u32) % 1000) * 1000000;
            Duration::new(seconds, nanos)
        }
    }

    /// Adds a given number of milliseconds to the Instant
    pub fn add_millis(&mut self, millis: u32) {
        let millis_f64: f64 = millis.into();
        self.inner += millis_f64;
    }

    /// Subtracts a given number of milliseconds to the Instant
    pub fn subtract_millis(&mut self, millis: u32) {
        let millis_f64: f64 = millis.into();
        self.inner -= millis_f64;
    }
}

impl Eq for Instant {}

#[allow(clippy::derive_ord_xor_partial_ord)]
impl Ord for Instant {
    fn cmp(&self, other: &Self) -> Ordering {
        // TODO: Use epsilon?
        if self.inner == other.inner {
            Ordering::Equal
        } else if self.inner < other.inner {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }
}
