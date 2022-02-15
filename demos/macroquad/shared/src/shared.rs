use std::time::Duration;

use naia_shared::{LinkConditionerConfig, SharedConfig};

use super::protocol::Protocol;

pub fn shared_config() -> SharedConfig<Protocol> {
    // Set tick rate to ~60 FPS
    let tick_interval = Some(Duration::from_millis(16));

    //let link_condition = None;
    let link_condition = Some(LinkConditionerConfig::average_condition());
    //let link_condition = Some(LinkConditionerConfig {
    //    incoming_latency: 500,
    //    incoming_jitter: 50,
    //    incoming_loss: 0.1,
    //});
    return SharedConfig::new(Protocol::load(), tick_interval, link_condition);
}
