use std::time::Duration;

use naia_shared::{LinkConditionerConfig, SharedConfig};

pub fn get_shared_config() -> SharedConfig {
    let tick_interval = Duration::from_millis(100);

    // Simulate network conditions with this configuration property
    let link_condition = Some(LinkConditionerConfig::average_condition());
    return SharedConfig::new(tick_interval, link_condition);
}
