use std::time::Duration;

use naia_shared::{LinkConditionerConfig, SharedConfig, SocketConfig};

use super::channels::{Channels, CHANNEL_CONFIG};

pub fn shared_config() -> SharedConfig<Channels> {
    // Set tick rate to ~60 FPS
    let tick_interval = Some(Duration::from_millis(20));

    //let link_condition = None;
    let link_condition = Some(LinkConditionerConfig::average_condition());
    // let link_condition = Some(LinkConditionerConfig {
    //     incoming_latency: 150,
    //     incoming_jitter: 50,
    //     incoming_loss: 0.1,
    // });

    SharedConfig::new(
        SocketConfig::new(link_condition, None),
        CHANNEL_CONFIG,
        tick_interval,
        None,
    )
}
