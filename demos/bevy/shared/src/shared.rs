use std::time::Duration;

use naia_shared::{ChannelConfig, DefaultChannels, LinkConditionerConfig, SharedConfig, SocketConfig};

pub fn shared_config() -> SharedConfig<DefaultChannels> {
    let tick_interval = Some(Duration::from_millis(50));

    //  let link_condition = None;
    let link_condition = Some(LinkConditionerConfig::average_condition());
    //  let link_condition = Some(LinkConditionerConfig {
    //      incoming_latency: 500,
    //      incoming_jitter: 1,
    //      incoming_loss: 0.0,
    //  });
    return SharedConfig::new(
        SocketConfig::new(link_condition, None),
        &ChannelConfig::<DefaultChannels>::default(),
        tick_interval,
        None);
}
