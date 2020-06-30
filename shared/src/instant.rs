use crate::duration::Duration;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        // Wasm //

        use js_sys::Date;

        /// Represents a specific moment in time
        #[derive(Debug, Clone)]
        pub struct Instant {
            inner: f64,
        }

        /// Creates an Instant from the moment the method is called
        impl Instant {
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

            /// Returns time elapsed since the Instant
            pub fn elapsed(&self) -> Duration {
                let inner_duration = self.inner.elapsed();
                return Duration::new(inner_duration.as_secs(), inner_duration.subsec_nanos());
            }
        }
    }
}
