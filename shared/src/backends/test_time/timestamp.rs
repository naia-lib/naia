use naia_socket_shared::TestClock;

pub struct Timestamp;

impl Timestamp {
    /// Returns the current simulated time as seconds since a fixed epoch
    /// 
    /// In tests, we use a fixed epoch (0) and convert simulated milliseconds to seconds.
    /// This ensures deterministic timestamps that advance with simulated time.
    /// 
    /// This uses `TestClock::current_time_ms()` which reads from the simulated clock,
    /// ensuring timestamps stay in sync with the test time abstraction.
    pub fn now() -> u64 {
        // Get current simulated time in milliseconds and convert to seconds
        // Use a fixed epoch (0) for deterministic test timestamps
        TestClock::current_time_ms() / 1000
    }
}
