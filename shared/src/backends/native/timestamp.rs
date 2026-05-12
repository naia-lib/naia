use std::time::SystemTime;

/// Returns the current wall-clock time as seconds since the Unix epoch.
pub struct Timestamp;

impl Timestamp {
    /// Returns seconds since the Unix epoch as a `u64`.
    pub fn now() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("timing error!")
            .as_secs()
    }
}
