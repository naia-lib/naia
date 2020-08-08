use std::time::Duration;

use naia_shared::{LinkConditionerConfig, SharedConfig};

pub fn get_shared_config() -> SharedConfig {
    let tick_interval = Duration::from_millis(1000);

    // Simulate network conditions with this configuration property
    //let link_condition = Some(LinkConditionerConfig::new(1200, 600, 0.01,
    // 0.001)); let link_condition =
    // Some(LinkConditionerConfig::average_condition());
    let link_condition = Some(LinkConditionerConfig::poor_condition());
    return SharedConfig::new(tick_interval, link_condition);
}
