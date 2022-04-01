use std::time::Duration;

use naia_shared::{
    ChannelConfig, DefaultChannels, LinkConditionerConfig, SharedConfig, SocketConfig,
};

pub fn shared_config() -> SharedConfig<DefaultChannels> {
    let tick_interval = Some(Duration::from_millis(800));

    // Simulate network conditions with this configuration property
    //let link_condition = Some(LinkConditionerConfig::average_condition());

    let link_condition = Some(LinkConditionerConfig {
        incoming_latency: 250,
        incoming_jitter: 200,
        incoming_loss: 0.05,
    });

    return SharedConfig::new(
        SocketConfig::new(link_condition, None),
        ChannelConfig::default(),
        tick_interval,
        None,
    );
}
