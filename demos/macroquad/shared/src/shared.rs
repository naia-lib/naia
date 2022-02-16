use std::time::Duration;

use naia_shared::{LinkConditionerConfig, SharedConfig};

use super::protocol::Protocol;

pub fn shared_config() -> SharedConfig<Protocol> {
    // Set tick rate to ~60 FPS
    let tick_interval = Some(Duration::from_millis(20));

    //let link_condition = None;
    //let link_condition = Some(LinkConditionerConfig::average_condition());
    let link_condition = Some(LinkConditionerConfig {
       incoming_latency: 100,
       incoming_jitter: 1,
       incoming_loss: 0.0,
    });
    return SharedConfig::new(Protocol::load(), tick_interval, link_condition);
}
