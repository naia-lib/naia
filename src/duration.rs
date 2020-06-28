
pub struct Duration {
    seconds: u64,
    nanos: u32,
}

impl Duration {
    pub fn new(seconds: u64, nanos: u32) -> Self {
        Duration {
            seconds,
            nanos,
        }
    }

    pub fn as_secs(&self) -> u64 {
        return self.seconds;
    }

    pub fn subsec_nanos(&self) -> u32 {
        return self.nanos;
    }
}