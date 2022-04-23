use std::time::Duration;

/// Represents a specific moment in time
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Instant {
    inner: std::time::Instant,
}

impl Instant {
    /// Creates an Instant from the moment the method is called
    pub fn now() -> Self {
        Instant {
            inner: std::time::Instant::now(),
        }
    }

    /// Returns time elapsed since the Instant
    pub fn elapsed(&self) -> Duration {
        self.inner.elapsed()
    }

    /// Returns time until the Instant occurs
    pub fn until(&self) -> Duration {
        self.inner.duration_since(std::time::Instant::now())
    }

    /// Adds a given number of milliseconds to the Instant
    pub fn add_millis(&mut self, millis: u32) {
        self.inner += Duration::from_millis(millis.into());
    }

    /// Subtracts a given number of milliseconds to the Instant
    pub fn subtract_millis(&mut self, millis: u32) {
        self.inner -= Duration::from_millis(millis.into());
    }

    /// Returns inner Instant implementation
    pub fn inner(&self) -> std::time::Instant {
        self.inner
    }
}
