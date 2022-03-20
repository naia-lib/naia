use std::time::SystemTime;

/// A Timestamp for a moment in time that can be read/written to/from a byte
/// stream
#[derive(Copy, Clone, PartialEq)]
pub struct Timestamp {
    time: u64,
}

impl Timestamp {
    /// Get a Timestamp for the current moment
    pub fn now() -> Self {
        let time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("timing error!")
            .as_secs();
        Timestamp { time }
    }

    /// Convert to u64
    pub fn to_u64(&self) -> u64 {
        self.time
    }

    /// Convert from u64
    pub fn from_u64(value: &u64) -> Self {
        Self { time: *value }
    }
}
