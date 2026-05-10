extern "C" {
    pub fn naia_now() -> f64;
}

pub struct Timestamp;

impl Timestamp {
    pub fn now() -> u64 {
        // Safety: naia_now() is an extern "C" function provided by the miniquad JavaScript
        // runtime. It returns the current time in milliseconds as a double with no side-effects
        // and no preconditions. wasm32 is single-threaded so there are no data races.
        unsafe { naia_now() as u64 }
    }
}
