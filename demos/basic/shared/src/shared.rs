use std::time::Duration;

use naia_shared::{
    ChannelConfig, DefaultChannels, LinkConditionerConfig, SharedConfig, SocketConfig,
};

use super::protocol::Protocol;

pub fn shared_config() -> SharedConfig<Protocol, DefaultChannels> {
    let tick_interval = Some(Duration::from_millis(1000));

    // Simulate network conditions with this configuration property
    //let link_condition = Some(LinkConditionerConfig::average_condition());

    let link_condition = Some(LinkConditionerConfig {
        incoming_latency: 750,
        incoming_jitter: 500,
        incoming_loss: 0.8,
    });

    return SharedConfig::new(
        Protocol::load(),
        SocketConfig::new(link_condition, None),
        ChannelConfig::default(),
        tick_interval,
        None,
    );
}
