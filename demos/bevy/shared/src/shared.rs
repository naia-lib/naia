use std::time::Duration;

use naia_shared::{LinkConditionerConfig, SharedConfig};

use super::protocol::Protocol;

pub fn get_shared_config() -> SharedConfig<Protocol> {
    let tick_interval = Some(Duration::from_millis(50));

    //    let link_condition = None;
    let link_condition = Some(LinkConditionerConfig::average_condition());
    //    let link_condition = Some(LinkConditionerConfig {
    //        incoming_latency: 500,
    //        incoming_jitter: 1,
    //        incoming_loss: 0.0,
    //    });
    return SharedConfig::new(Protocol::load(), tick_interval, link_condition);
}
