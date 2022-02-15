use std::time::Duration;

use naia_shared::{LinkConditionerConfig, SharedConfig};

use super::protocol::Protocol;

pub fn get_shared_config() -> SharedConfig<Protocol> {
    let tick_interval = Some(Duration::from_millis(50));

    // Simulate network conditions with this configuration property
    let link_condition = Some(LinkConditionerConfig::average_condition());
    return SharedConfig::new(Protocol::load(), tick_interval, link_condition);
}
