use std::time::Duration;

//TODO: Timestamp & Instant implementations can probably be merged

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        // Wasm //

        use js_sys::Date;

        /// Represents a specific moment in time
        #[derive(Debug, Clone)]
        pub struct Instant {
            inner: f64,
        }

        impl Instant {
            /// Creates an Instant from the moment the method is called
            pub fn now() -> Self {
                Instant {
                    inner: Date::now(),
                }
            }

            /// Returns time elapsed since the Instant
            pub fn elapsed(&self) -> Duration {
                let inner_duration = Date::now() - self.inner;
                let seconds: u64 = (inner_duration as u64) / 1000;
                let nanos: u32 = ((inner_duration as u32) % 1000) * 1000000;
                return Duration::new(seconds, nanos);
            }

            /// Returns the duration since a previous Instant
            pub fn duration_since(&self, earlier: &Instant) -> Duration {
                let inner_duration = self.inner - earlier.inner;
                let seconds: u64 = (inner_duration as u64) / 1000;
                let nanos: u32 = ((inner_duration as u32) % 1000) * 1000000;
                return Duration::new(seconds, nanos);
            }

            /// Sets the Instant to the value of another
            pub fn set_to(&mut self, other: &Instant) {
                self.inner = other.inner.clone();
            }
        }
    }
    else {
        // Linux //
        /// Represents a specific moment in time
        #[derive(Debug, Clone)]
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

            /// Returns current time elapsed since the Instant
            pub fn elapsed(&self) -> Duration {
                let inner_duration = self.inner.elapsed();
                return Duration::new(inner_duration.as_secs(), inner_duration.subsec_nanos());
            }

            /// Returns the duration since a previous Instant
            pub fn duration_since(&self, earlier: &Instant) -> Duration {
                let inner_duration = self.inner.saturating_duration_since(earlier.inner);
                return Duration::new(inner_duration.as_secs(), inner_duration.subsec_nanos());
            }

            /// Sets the Instant to the value of another
            pub fn set_to(&mut self, other: &Instant) {
                self.inner = other.inner.clone();
            }

            /// Adds a Duration to the Instant, if allowed
            pub fn add(&mut self, duration: &Duration) {
                if let Some(result) = self.inner.checked_add(*duration) {
                    self.inner = result;
                }
            }

            /// Adds a Duration to the Instant, if allowed
            pub fn sub(&mut self, duration: &Duration) {
                if let Some(result) = self.inner.checked_sub(*duration) {
                    self.inner = result;
                }
            }
        }
    }
}
