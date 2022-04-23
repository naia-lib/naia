use std::time::Duration;

use naia_shared::{
    ChannelConfig, DefaultChannels, LinkConditionerConfig, SharedConfig, SocketConfig,
};

pub fn shared_config() -> SharedConfig<DefaultChannels> {
    let tick_interval = Some(Duration::from_millis(25));

    // Simulate network conditions with this configuration property
    let link_condition = Some(LinkConditionerConfig {
        incoming_latency: 350,
        incoming_jitter: 345,
        incoming_loss: 0.5,
    });
    return SharedConfig::new(
        SocketConfig::new(link_condition, None),
        ChannelConfig::<DefaultChannels>::default(),
        tick_interval,
        None,
    );
}
