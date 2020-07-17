use naia_socket_shared::LinkConditionerConfig;
use std::{default::Default, time::Duration};

/// Contains Config properties which will be shared by Server and Client
#[derive(Clone, Debug)]
pub struct SharedConfig {
    /// The duration between each tick
    pub tick_interval: Duration,
    /// Configuration used to simulate network conditions
    pub link_condition_config: Option<LinkConditionerConfig>,
}

impl SharedConfig {
    /// Creates a new SharedConfig
    pub fn new(
        tick_interval: Duration,
        link_condition_config: Option<LinkConditionerConfig>,
    ) -> Self {
        SharedConfig {
            tick_interval,
            link_condition_config,
        }
    }
}

impl Default for SharedConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_secs(1),
            link_condition_config: None,
        }
    }
}
