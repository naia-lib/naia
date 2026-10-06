use std::{cmp::Ordering, time::Duration};

use wasm_bindgen::JsCast;

/// Represents a specific moment in time
#[derive(Clone, PartialEq, PartialOrd)]
pub struct Instant {
    inner: f64,
}

/// Monotonic milliseconds from the web performance timeline.
///
/// DWO monotonic clock: this is `performance.now()`, which the platform
/// guarantees never moves backwards when the system wall clock is adjusted
/// (hr-time-3). It deliberately does NOT fall back to `Date::now()`: a
/// silent fallback would reintroduce wall-clock steps exactly where
/// monotonicity is required. Measuring across an OS suspend is out of
/// scope — the server lease is the authority and the client reconciles
/// with it on resume.
///
/// This is the one monotonic source behind the wasm time facade: both
/// [`Instant`] and naia-shared's wasm `Timer` read it, so the two never
/// disagree about elapsed time.
#[must_use]
pub fn monotonic_now_ms() -> f64 {
    let global = js_sys::global();
    let performance = js_sys::Reflect::get(&global, &"performance".into())
        .expect("web performance timeline is available");
    let now_fn =
        js_sys::Reflect::get(&performance, &"now".into()).expect("performance.now is available");
    let now_fn: &js_sys::Function = now_fn.dyn_ref().expect("performance.now is callable");
    now_fn
        .call0(&performance)
        .expect("performance.now returns")
        .as_f64()
        .expect("performance.now returns milliseconds")
}

impl Instant {
    /// Creates an Instant from the moment the method is called
    #[must_use]
    pub fn now() -> Self {
        Instant {
            inner: monotonic_now_ms(),
        }
    }

    /// Returns time elapsed since the Instant
    #[must_use]
    pub fn elapsed(&self, now: &Self) -> Duration {
        let inner_duration = now.inner - self.inner;
        let seconds: u64 = (inner_duration as u64) / 1000;
        let nanos: u32 = ((inner_duration as u32) % 1000) * 1000000;
        Duration::new(seconds, nanos)
    }

    /// Returns time until the Instant occurs
    #[must_use]
    pub fn until(&self, now: &Self) -> Duration {
        let inner_duration = self.inner - now.inner;
        let seconds: u64 = (inner_duration as u64) / 1000;
        let nanos: u32 = ((inner_duration as u32) % 1000) * 1000000;
        Duration::new(seconds, nanos)
    }

    /// Returns whether the Instant is after another Instant
    #[must_use]
    pub fn is_after(&self, other: &Self) -> bool {
        self.inner > other.inner
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

// DWO monotonic clock: with an injected backwards wall-clock jump, elapsed
// time must never decrease. The test patches the global Date (the old
// wall-clock source) backwards by an hour between two reads; a monotonic
// source does not follow it. Date is restored before asserting so a
// failure cannot leak the patch into sibling tests.
#[cfg(all(test, target_arch = "wasm32"))]
mod wall_jump_tests {
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::Instant;

    fn patch_date_back_one_hour() {
        js_sys::eval(
            "Date.now = (() => { const real = Date.now.bind(Date); return () => real() - 3600000; })()",
        )
        .expect("patch Date.now backwards");
    }

    fn restore_date() {
        js_sys::eval("delete Date.now").expect("restore Date.now");
    }

    #[wasm_bindgen_test]
    fn backwards_wall_jump_never_decreases_elapsed() {
        let before = Instant::now();
        patch_date_back_one_hour();
        let after = Instant::now();
        let ordered = after >= before;
        restore_date();
        // `elapsed` derives from the same subtraction, so ordering pins it:
        // a wall-clock jump must never move the clock backwards.
        assert!(
            ordered,
            "a one-hour backwards wall-clock jump moved Instant backwards"
        );
    }

    // DWO monotonic clock, sampling side: consecutive reads of the live
    // performance.now timeline never step backwards, with no sleeps and no
    // injected jumps — the facade holds under ordinary sampling too.
    #[wasm_bindgen_test]
    fn successive_nows_never_go_backwards() {
        let mut previous = Instant::now();
        for _ in 0..1000 {
            let current = Instant::now();
            assert!(
                current >= previous,
                "successive performance.now reads stepped backwards"
            );
            previous = current;
        }
    }
}
