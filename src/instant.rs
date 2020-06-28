
use crate::Duration;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        /// Wasm ///

        use js_sys::Date;

        #[derive(Clone)]
        pub struct Instant {
            inner: f64,
        }

        impl Instant {
            pub fn now() -> Self {
                Instant {
                    inner: Date::now(),
                }
            }

            pub fn elapsed(&self) -> Duration {
                let inner_duration = Date::now() - self.inner;
                let seconds: u64 = (inner_duration as u64) / 1000;
                let nanos: u32 = ((inner_duration as u32) % 1000) * 1000000;
                return Duration::new(seconds, nanos);
            }
        }
    }
    else {
        /// Linux ///

        #[derive(Clone)]
        pub struct Instant {
            inner: std::time::Instant,
        }

        impl Instant {
            pub fn now() -> Self {
                Instant {
                    inner: std::time::Instant::now(),
                }
            }

            pub fn elapsed(&self) -> Duration {
                let inner_duration = self.inner.elapsed();
                return Duration::new(inner_duration.as_secs(), inner_duration.subsec_nanos());
            }
        }
    }
}