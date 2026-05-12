use naia_socket_shared::TestClock;

#[doc(hidden)]
pub struct Timestamp;

impl Timestamp {
    /// Returns the current simulated time in milliseconds (test clock, not wall clock).
    pub fn now() -> u64 {
        TestClock::current_time_ms() / 1000
    }
}
