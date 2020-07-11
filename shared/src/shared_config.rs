use std::{default::Default, time::Duration};

/// Contains Config properties which will be shared by Server and Client
#[derive(Clone, Debug)]
pub struct SharedConfig {
    /// The duration between each tick
    pub tick_interval: Duration,
}

impl SharedConfig {
    /// Creates a new SharedConfig
    pub fn new(tick_interval: Duration) -> Self {
        SharedConfig { tick_interval }
    }
}

impl Default for SharedConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(1),
        }
    }
}
