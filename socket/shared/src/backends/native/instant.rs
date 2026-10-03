use std::time::Duration;

/// Represents a specific moment in time
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Instant {
    inner: std::time::Instant,
}

impl Instant {
    /// Creates an Instant from the moment the method is called
    #[must_use]
    pub fn now() -> Self {
        Self {
            inner: std::time::Instant::now(),
        }
    }

    /// Returns time elapsed since the Instant
    #[must_use]
    pub fn elapsed(&self, now: &Self) -> Duration {
        now.inner - self.inner
    }

    /// Returns time until the Instant occurs
    #[must_use]
    pub fn until(&self, now: &Self) -> Duration {
        self.inner.duration_since(now.inner())
    }

    #[must_use]
    pub fn is_after(&self, other: &Self) -> bool {
        self.inner > other.inner
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
    #[must_use]
    pub fn inner(&self) -> std::time::Instant {
        self.inner
    }
}
